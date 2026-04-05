use crate::target::Target;

macro_rules! define_lang_items {
    (
        $($variant:ident, $name:ident, $target:expr,)*
    ) => {
        #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
        pub enum LangItem {
            $($variant,)*
        }

        impl LangItem {
            pub const COUNT: usize = {
                let _all = [ $(stringify!($name),)* ];
                _all.len()
            };
            /// 从字符串切片获取 LangItem 枚举值
            pub fn from_name(s: &str) -> Option<Self> {
                match s {
                    $(stringify!($name) => Some(LangItem::$variant),)*
                    _ => None,
                }
            }

            /// 获取 LangItem 对应的字符串切片
            pub fn name(self) -> &'static str {
                match self {
                    $(LangItem::$variant => stringify!($name),)*
                }
            }

            pub fn target(self) -> Target {
                match self {
                    $(LangItem::$variant => $target,)*
                }
            }
        }
    };
}

// 使用
define_lang_items! {
    Add,            add,            Target::Trait,
    Sub,            sub,            Target::Trait,
    Mul,            mul,            Target::Trait,
    Div,            div,            Target::Trait,
    Rem,            rem,            Target::Trait,
    Neg,            neg,            Target::Trait,
    Not,            not,            Target::Trait,
    BitAnd,         bit_and,        Target::Trait,
    BitOr,          bit_or,         Target::Trait,
    BitXor,         bit_xor,        Target::Trait,
    Shl,            shl,            Target::Trait,
    Shr,            shr,            Target::Trait,
    Index,          index,          Target::Trait,
    IndexMut,       index_mut,      Target::Trait,
    Drop,           drop,           Target::Trait,
    Clone,          clone,          Target::Trait,
    Copy,           copy,           Target::Trait,

    Panic,          panic,          Target::Fn,
}
