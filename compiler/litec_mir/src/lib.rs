use litec_hir::{AbiType, BinOp, LiteralValue, Mutability, UnOp, Visibility};
use litec_span::{Span, StringId};
use litec_typed_hir::{CastKind, Definition, TypedField, builtins::Builtin, def_id::DefId, ty::Ty};
use rustc_hash::FxHashMap;

#[derive(Debug)]
pub struct MirCrate {
    pub items: Vec<MirItem>,
    pub globals: FxHashMap<DefId, GlobalDecl>,
    pub builtin: Builtin,
    pub definitions: Vec<Definition>,
}

impl MirCrate {
    pub fn new(builtin: Builtin, definitions: Vec<Definition>) -> Self {
        MirCrate {
            items: Vec::new(),
            globals: FxHashMap::default(),
            builtin: builtin,
            definitions: definitions,
        }
    }

    pub fn add_item(&mut self, item: MirItem) {
        self.items.push(item);
    }

    pub fn get_function(&self, def_id: DefId) -> Option<&MirFunction> {
        self.items.iter().find_map(|item| match item {
            MirItem::Function(func) if func.def_id == def_id => Some(func),
            _ => None,
        })
    }
}

#[derive(Debug)]
pub enum MirItem {
    Function(MirFunction),
    Struct(MirStruct),
    Use(MirUse),
    Module(MirModule),
    Extern(MirExtern),
}

#[derive(Debug, Clone)]
pub struct MirFunction {
    pub def_id: DefId,
    pub local_decls: Vec<LocalDecl>,
    pub basic_blocks: Vec<BasicBlock>,
    pub args: Vec<Local>,
    pub return_ty: Ty,
    pub span: Span,
}

#[derive(Debug)]
pub struct MirStruct {
    pub def_id: DefId,
    pub visibility: Visibility,
    pub name: StringId,
    pub fields: Vec<MirField>,
    pub span: Span,
}

#[derive(Debug)]
pub struct MirField {
    pub def_id: DefId,
    pub name: StringId,
    pub ty: Ty,
    pub visibility: Visibility,
    pub span: Span,
}

#[derive(Debug)]
pub struct MirUse {
    pub def_id: DefId,
    pub visibility: Visibility,
    pub alias: StringId,
    pub target: DefId,
    pub span: Span,
}

#[derive(Debug)]
pub struct MirModule {
    pub def_id: DefId,
    pub visibility: Visibility,
    pub name: StringId,
    pub items: Vec<MirItem>,
    pub span: Span,
}

#[derive(Debug)]
pub struct MirExtern {
    pub def_id: DefId,
    pub visibility: Visibility,
    pub name: StringId,
    pub abi: AbiType,
    pub items: Vec<MirExternItem>,
    pub span: Span,
}

#[derive(Debug)]
pub enum MirExternItem {
    Function(MirExternFunction),
}

#[derive(Debug)]
pub struct MirExternFunction {
    pub def_id: DefId,
    pub name: StringId,
    pub params: Vec<MirParam>,
    pub is_variadic: bool,
    pub return_ty: Option<Ty>,
    pub span: Span,
}

#[derive(Debug)]
pub struct MirParam {
    pub def_id: DefId,
    pub name: StringId,
    pub ty: Ty,
    pub span: Span,
}

impl MirFunction {
    pub fn new_basic_block(&mut self, span: Span) -> BasicBlockId {
        let id = BasicBlockId(self.basic_blocks.len());
        self.basic_blocks.push(BasicBlock {
            statements: Vec::new(),
            terminator: Terminator {
                kind: TerminatorKind::Return {
                    value: Operand::Constant(Constant {
                        kind: ConstantKind::Literal {
                            value: LiteralValue::Unit,
                            ty: Ty::Unit,
                        },
                        span: span,
                    }),
                    is_explicit: false,
                },
                span: span,
            },
        });
        id
    }

    pub fn basic_block_mut(&mut self, id: BasicBlockId) -> &mut BasicBlock {
        &mut self.basic_blocks[id.0]
    }

    pub fn basic_block(&self, id: BasicBlockId) -> &BasicBlock {
        &self.basic_blocks[id.0]
    }
}

#[derive(Debug, Clone)]
pub struct LocalDecl {
    pub ty: Ty,
    pub mutability: Mutability,
    pub name: Option<StringId>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct GlobalDecl {
    pub def_id: DefId,
    pub name: StringId,
    pub ty: Ty,
    pub init: Option<Constant>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Local(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BasicBlockId(pub usize);

#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone)]
pub struct Statement {
    pub span: Span,
    pub kind: StatementKind,
}

#[derive(Debug, Clone)]
pub enum StatementKind {
    Assign { place: Place, rvalue: Rvalue },
    Nop,
}

#[derive(Debug, Clone)]
pub enum Rvalue {
    Use(Operand),
    BinaryOp(BinOp, Operand, Operand),
    UnaryOp(UnOp, Operand),
    Ref(Mutability, Place),
    Deref(Place),
    /// 聚合值，可以是数组、元组等，包含聚合类型和操作数列表
    Aggregate(AggregateKind, Vec<Operand>),
    /// 获取地址操作，作用于一个位置
    AddressOf(Place),
    Cast(CastKind, Place),
}

/// 聚合类型，表示可以包含多个值的复合类型
#[derive(Debug, Clone)]
pub enum AggregateKind {
    /// 数组类型，包含元素类型
    Array(Ty),
    /// 元组类型
    Tuple,
    /// 自定义数据类型（ADT），包含定义ID和变体索引列表
    Adt(DefId, Vec<usize>),
}

#[derive(Debug, Clone, Copy)]
pub enum Movability {
    Static,
    Movable,
}

#[derive(Debug, Clone)]
pub enum Operand {
    Copy(Place),
    Move(Place),
    Constant(Constant),
}

#[derive(Debug, Clone)]
pub struct Constant {
    pub kind: ConstantKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum ConstantKind {
    Literal { value: LiteralValue, ty: Ty },
    Global { def_id: DefId, ty: Ty },
    Function { def_id: DefId, ty: Ty },
    Unit,
}

#[derive(Debug, Clone)]
pub struct Terminator {
    pub kind: TerminatorKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum TerminatorKind {
    Return {
        value: Operand,
        is_explicit: bool,
    },
    Goto {
        target: BasicBlockId,
    },
    SwitchInt {
        discr: Operand,
        targets: SwitchTargets,
    },
    Call {
        function: DefId,
        args: Vec<Operand>,
        destination: Place,
        target: BasicBlockId,
    },
}

#[derive(Debug, Clone)]
pub enum AssertMessage {
    BoundsCheck { len: Operand, index: Operand },
    Overflow(BinOp, Operand, Operand),
    OverflowNeg(Operand),
    DivisionByZero(Operand),
    RemainderByZero(Operand),
    ResumedAfterReturn(GeneratorKind),
    ResumedAfterPanic(GeneratorKind),
}

#[derive(Debug, Clone, Copy)]
pub enum GeneratorKind {
    Gen,
    Async,
}

#[derive(Debug, Clone)]
pub struct Place {
    pub local: Local,
    pub projection: Vec<PlaceElem>,
    pub ty: Ty,
}

#[derive(Debug, Clone)]
pub enum PlaceElem {
    Deref,
    Field(TypedField),
    Index(Local),
    ConstantIndex {
        offset: u64,
        min_length: u64,
        from_end: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: StringId,
    pub ty: Ty,
    pub index: u32,
}

#[derive(Debug, Clone)]
pub struct SwitchTargets {
    pub values: Vec<u128>,
    pub targets: Vec<BasicBlockId>,
    pub otherwise: BasicBlockId,
}

impl Place {
    pub fn local(local: Local, ty: Ty) -> Self {
        Place {
            local,
            projection: Vec::new(),
            ty: ty,
        }
    }

    pub fn field(local: Local, field: TypedField, ty: Ty) -> Self {
        Place {
            local,
            projection: vec![PlaceElem::Field(field)],
            ty: ty,
        }
    }

    pub fn deref(mut self) -> Self {
        self.projection.push(PlaceElem::Deref);
        self
    }

    pub fn field_access(mut self, field: TypedField) -> Self {
        self.projection.push(PlaceElem::Field(field));
        self
    }

    pub fn index(mut self, local: Local) -> Self {
        self.projection.push(PlaceElem::Index(local));
        self
    }
}

impl SwitchTargets {
    pub fn if_else(then_block: BasicBlockId, else_block: BasicBlockId) -> Self {
        SwitchTargets {
            values: vec![0],
            targets: vec![else_block, then_block],
            otherwise: else_block,
        }
    }

    pub fn match_branch(
        values: Vec<u128>,
        targets: Vec<BasicBlockId>,
        otherwise: BasicBlockId,
    ) -> Self {
        SwitchTargets {
            values,
            targets,
            otherwise,
        }
    }
}

impl StatementKind {
    pub fn assign(place: Place, rvalue: Rvalue) -> Self {
        StatementKind::Assign { place, rvalue }
    }

    pub fn nop() -> Self {
        StatementKind::Nop
    }
}

impl Rvalue {
    pub fn use_operand(operand: Operand) -> Self {
        Rvalue::Use(operand)
    }

    pub fn binary_op(op: BinOp, left: Operand, right: Operand) -> Self {
        Rvalue::BinaryOp(op, left, right)
    }

    pub fn unary_op(op: UnOp, operand: Operand) -> Self {
        Rvalue::UnaryOp(op, operand)
    }

    pub fn reference(mutability: Mutability, place: Place) -> Self {
        Rvalue::Ref(mutability, place)
    }

    pub fn dereference(place: Place) -> Self {
        Rvalue::Deref(place)
    }

    pub fn address_of(place: Place) -> Self {
        Rvalue::AddressOf(place)
    }
}

impl Operand {
    pub fn copy(place: Place) -> Self {
        Operand::Copy(place)
    }

    pub fn move_from(place: Place) -> Self {
        Operand::Move(place)
    }

    pub fn constant(value: Constant) -> Self {
        Operand::Constant(value)
    }
}

impl Terminator {
    pub fn ret(value: Operand, is_explicit: bool, span: Span) -> Self {
        Terminator {
            kind: TerminatorKind::Return { value, is_explicit },
            span: span,
        }
    }

    pub fn goto(target: BasicBlockId, span: Span) -> Self {
        Terminator {
            kind: TerminatorKind::Goto { target },
            span: span,
        }
    }

    pub fn call(
        function: DefId,
        args: Vec<Operand>,
        destination: Place,
        target: BasicBlockId,
        span: Span,
    ) -> Self {
        Terminator {
            kind: TerminatorKind::Call {
                function,
                args,
                destination,
                target,
            },
            span: span,
        }
    }
}
