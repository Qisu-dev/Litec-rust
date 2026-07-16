use litec_middle::ty::{Ty, TypeVarId};
use rustc_hash::{FxHashMap, FxHashSet};

pub type Subst = FxHashMap<TypeVarId, Ty>;

#[derive(Debug, Clone)]
pub struct PolyTy {
    pub vars: Vec<TypeVarId>,
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub struct TypeEnv {
    pub subst: Subst,
    pub next_var: usize,
}

impl TypeEnv {
    pub fn new() -> Self {
        Self {
            subst: FxHashMap::default(),
            next_var: 0,
        }
    }

    pub fn new_var(&mut self) -> Ty {
        let id = TypeVarId(self.next_var);
        self.next_var += 1;
        Ty::Var(id)
    }

    pub fn unify(&mut self, a: &Ty, b: &Ty) -> Result<(), String> {
        let a = self.apply_subst(a);
        let b = self.apply_subst(b);
        match (&a, &b) {
            (Ty::Var(id1), Ty::Var(id2)) if id1 == id2 => Ok(()),
            (Ty::Var(id), ty) | (ty, Ty::Var(id)) => {
                if self.occurs(*id, &ty) {
                    return Err(format!("occurs check failed for {:?}", id));
                }
                self.subst.insert(*id, ty.clone());
                Ok(())
            }
            (Ty::Prim(p1), Ty::Prim(p2)) if p1 == p2 => Ok(()),
            (Ty::Adt(d1, args1), Ty::Adt(d2, args2)) if d1 == d2 => {
                if args1.len() != args2.len() {
                    return Err("ADT arity mismatch".to_string());
                }
                for (a1, a2) in args1.iter().zip(args2.iter()) {
                    self.unify(a1, a2)?;
                }
                Ok(())
            }
            (Ty::Fn(params1, ret1), Ty::Fn(params2, ret2)) => {
                if params1.len() != params2.len() {
                    return Err("function arity mismatch".to_string());
                }
                for (a, b) in params1.iter().zip(params2.iter()) {
                    self.unify(a, b)?;
                }
                self.unify(ret1.as_ref(), ret2.as_ref())?;
                Ok(())
            }
            (Ty::Ptr(inner1), Ty::Ptr(inner2)) => self.unify(inner1, inner2),
            (Ty::Tuple(elems1), Ty::Tuple(elems2)) => {
                if elems1.len() != elems2.len() {
                    return Err("tuple arity mismatch".to_string());
                }
                for (a, b) in elems1.iter().zip(elems2.iter()) {
                    self.unify(a, b)?;
                }
                Ok(())
            }
            (Ty::Never, _) | (_, Ty::Never) => {
                Ok(())
            }
            _ => Err(format!("unification failed: {:?} vs {:?}", a, b)),
        }
    }

    pub fn occurs(&self, id: TypeVarId, ty: &Ty) -> bool {
        match ty {
            Ty::Var(id2) => id == *id2,
            Ty::Fn(params, ret) => {
                params.iter().any(|p| self.occurs(id, p)) || self.occurs(id, ret)
            }
            Ty::Adt(_, args) => args.iter().any(|a| self.occurs(id, a)),
            Ty::Ptr(inner) => self.occurs(id, inner),
            Ty::Tuple(elems) => elems.iter().any(|e| self.occurs(id, e)),
            _ => false,
        }
    }

    pub fn apply_subst(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Var(id) => {
                if let Some(subst_ty) = self.subst.get(id) {
                    self.apply_subst(subst_ty)
                } else {
                    ty.clone()
                }
            }
            Ty::Fn(params, ret) => Ty::Fn(
                params.iter().map(|p| self.apply_subst(p)).collect(),
                Box::new(self.apply_subst(ret)),
            ),
            Ty::Adt(def_id, args) => {
                Ty::Adt(*def_id, args.iter().map(|a| self.apply_subst(a)).collect())
            }
            Ty::Ptr(inner) => Ty::Ptr(Box::new(self.apply_subst(inner))),
            Ty::Tuple(elems) => Ty::Tuple(elems.iter().map(|e| self.apply_subst(e)).collect()),
            _ => ty.clone(),
        }
    }

    pub fn generalize(&self, ty: &Ty, env_vars: &FxHashSet<TypeVarId>) -> PolyTy {
        let mut vars = FxHashSet::default();
        self.collect_vars(ty, &mut vars);
        let generalized = vars.difference(env_vars).cloned().collect();
        PolyTy {
            vars: generalized,
            ty: ty.clone(),
        }
    }

    pub fn instantiate(&mut self, poly: &PolyTy) -> Ty {
        let mut map = FxHashMap::default();
        for var in &poly.vars {
            map.insert(*var, self.new_var());
        }
        self.apply_subst_with(&poly.ty, &map)
    }

    fn apply_subst_with(&self, ty: &Ty, map: &FxHashMap<TypeVarId, Ty>) -> Ty {
        match ty {
            Ty::Var(id) => {
                if let Some(new_ty) = map.get(id) {
                    self.apply_subst_with(new_ty, map)
                } else {
                    ty.clone()
                }
            }
            Ty::Fn(params, ret) => Ty::Fn(
                params
                    .iter()
                    .map(|p| self.apply_subst_with(p, map))
                    .collect(),
                Box::new(self.apply_subst_with(ret, map)),
            ),
            Ty::Adt(def_id, args) => Ty::Adt(
                *def_id,
                args.iter().map(|a| self.apply_subst_with(a, map)).collect(),
            ),
            Ty::Ptr(inner) => Ty::Ptr(Box::new(self.apply_subst_with(inner, map))),
            Ty::Tuple(elems) => Ty::Tuple(
                elems
                    .iter()
                    .map(|e| self.apply_subst_with(e, map))
                    .collect(),
            ),
            _ => ty.clone(),
        }
    }

    pub fn collect_vars(&self, ty: &Ty, set: &mut FxHashSet<TypeVarId>) {
        match ty {
            Ty::Var(id) => {
                set.insert(*id);
            }
            Ty::Fn(params, ret) => {
                for p in params {
                    self.collect_vars(p, set);
                }
                self.collect_vars(ret, set);
            }
            Ty::Adt(_, args) => {
                for a in args {
                    self.collect_vars(a, set);
                }
            }
            Ty::Ptr(inner) => self.collect_vars(inner, set),
            Ty::Tuple(elems) => {
                for e in elems {
                    self.collect_vars(e, set);
                }
            }
            _ => {}
        }
    }
}
