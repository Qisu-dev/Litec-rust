use index_vec::{Idx, define_index_type};
use std::hash::Hash;

/// 标识一个 crate（当前或依赖）。
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct CrateNum(pub u32);

/// 当前 crate 的专用编号（通常为 0）。
pub const LOCAL_CRATE: CrateNum = CrateNum(0);

impl Idx for CrateNum {
    fn from_usize(idx: usize) -> Self {
        CrateNum(idx as u32)
    }
    fn index(self) -> usize {
        self.0 as usize
    }
}

/// crate 内部的定义索引（原始值类型）。
pub type DefIndex = u32;

// 用宏定义 LocalDefId，使其实现 Idx
define_index_type! {
    /// 当前 crate 内的定义标识符（省略 crate 部分），连续分配。
    pub struct LocalDefId = u32;
}

// 用宏定义 ItemLocalId，使其实现 Idx
define_index_type! {
    /// 在所有者内部的局部节点 ID，连续分配
    pub struct ItemLocalId = u32;
}

/// 所有者标识符，包装 LocalDefId，也实现 Idx。
/// 用于在 HIR 存储中作为第一维索引。
#[derive(Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct OwnerId(pub LocalDefId);

impl Idx for OwnerId {
    fn from_usize(idx: usize) -> Self {
        OwnerId(LocalDefId::from_usize(idx))
    }

    fn index(self) -> usize {
        self.0.index()
    }
}

impl From<LocalDefId> for OwnerId {
    fn from(def_id: LocalDefId) -> Self {
        OwnerId(def_id)
    }
}

// ========== 全局定义 ID ==========

/// 全局唯一的定义标识符（跨 crate）。
/// 通过 `CrateNum` 和 `DefIndex` 唯一确定一个定义。
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct DefId {
    pub krate: CrateNum,
    pub index: DefIndex,
}

impl DefId {
    pub fn default() -> Self {
        Self {
            krate: CrateNum::default(),
            index: Default::default(),
        }
    }
}

// ========== 转换方法 ==========

impl LocalDefId {
    /// 转换为全局 DefId（需提供当前 crate 编号）。
    pub fn to_def_id(self, krate: CrateNum) -> DefId {
        DefId {
            krate,
            index: self.index() as u32, // index() 返回 usize，转为 u32
        }
    }
}

impl OwnerId {
    /// 转换为全局 DefId（需提供当前 crate 编号）。
    pub fn to_def_id(self, krate: CrateNum) -> DefId {
        self.0.to_def_id(krate)
    }

    /// 获取内部的 LocalDefId。
    pub fn as_local_def_id(self) -> LocalDefId {
        self.0
    }
}

/// 完整的 HIR 节点 ID，由所有者 [`OwnerId`] 和局部 ID 组成。
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct HirId {
    pub owner: OwnerId,
    pub local_id: ItemLocalId,
}
