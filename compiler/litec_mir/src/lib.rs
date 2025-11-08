use litec_typed_hir::def_id::DefId;
use litec_span::{Span, StringId};

// ---------- 实体 ID ----------
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct LocalId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub struct BasicBlockId(pub usize);

// ---------- 类型 ----------
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    Int(IntKind),
    Float(FloatKind),
    Bool,
    Unit,
    Never, // !
    Unknown,
    Str
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntKind  { I8, I16, I32, I64, I128, Isize, U8, U16, U32, U64, U128, Usize }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatKind{ F32, F64 }

// ---------- 操作数 ----------
#[derive(Debug, Clone)]
pub enum Operand {
    Literal(Literal),
    Local(LocalId),
    Static(DefId),          // 全局 / static
}
#[derive(Debug, Clone)]
pub enum Literal {
    I8(i8), I16(i16), I32(i32), I64(i64), I128(i128), Isize(isize),
    U8(u8), U16(u16), U32(u32), U64(u64), U128(u128),Usize(usize), 
    F32(f32), F64(f64), Bool(bool), Unit, Never, Str(StringId), Char(char)
}

// ---------- 语句（不跳转） ----------
#[derive(Debug, Clone)]
pub enum Statement {
    Assign { dest: LocalId, rvalue: Rvalue, span: Span },
    // 以后可加 StorageLive / StorageDead / Drop / Deinit …
}
#[derive(Debug, Clone)]
pub enum Rvalue {
    Use(Operand),
    Binary(BinOp, Operand, Operand),
    CheckedBinary(BinOp, Operand, Operand), // 返回 (结果, bool)
    Unary(UnOp, Operand),
    Aggregate(AggregateKind, Vec<Operand>),
    Ref(bool /*mut*/, Place),               // & / &mut
    Len(Place),                              // [T; N].len()
    // 更多：Cast / Discriminant / Repeat …
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp { Add, Sub, Mul, Div, Rem, Eq, Lt, Le, Ne, Ge, Gt, And, Or }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp  { Not, Neg }
#[derive(Debug, Clone)]
pub enum AggregateKind {
    Tuple,
    Adt(DefId /*struct/enum*/),
    Array,
}

// 左值（内存位置）
#[derive(Debug, Clone)]
pub struct Place {
    pub base: PlaceBase,
    pub projections: Vec<PlaceElem>,
}
#[derive(Debug, Clone)]
pub enum PlaceBase {
    Local(LocalId),
    Static(DefId),
}
#[derive(Debug, Clone)]
pub enum PlaceElem {
    Field(usize),      // .0  .1
    Index(LocalId),    // [i]
    Deref,             // *
}

// ---------- 终结符（控制流） ----------
#[derive(Debug, Clone)]
pub enum Terminator {
    Goto { target: BasicBlockId, span: Span },
    Switch { discr: Operand, targets: SwitchTargets, span: Span },
    Return { value: Operand, span: Span },
    Unreachable { span: Span },
    // 以后可加 Call + cleanup, Assert, Yield, InlineAsm …
}
#[derive(Debug, Clone)]
pub struct SwitchTargets {
    pub values: Vec<u128>,          // 匹配的字面量
    pub targets: Vec<BasicBlockId>, // 一一对应
    pub otherwise: BasicBlockId,    // default
}

// ---------- 基本块 ----------
#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BasicBlockId,
    pub statements: Vec<Statement>,
    pub terminator: Option<Terminator>,
}
impl BasicBlock {
    pub fn new(id: BasicBlockId) -> Self {
        BasicBlock { id, statements: Vec::new(), terminator: None }
    }
}

// ---------- 函数 ----------
#[derive(Debug, Clone)]
pub struct MirFunction {
    pub def_id: DefId,
    pub name: StringId,
    pub locals: Vec<LocalDecl>, // local_decls[0] 是 return place
    pub basic_blocks: Vec<BasicBlock>,
    pub span: Span,
}
#[derive(Debug, Clone)]
pub struct LocalDecl {
    pub ty: Ty,
    pub name: Option<StringId>, // 调试名，可空
    pub span: Span,
}