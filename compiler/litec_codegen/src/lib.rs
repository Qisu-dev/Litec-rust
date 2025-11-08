pub mod linker;
use std::path::Path;

use inkwell::{
    builder::Builder, context::Context, module::Module, targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine}, types::{BasicType, BasicTypeEnum}, values::{BasicValueEnum, FunctionValue, PointerValue}, AddressSpace, FloatPredicate, IntPredicate, OptimizationLevel
};
use litec_span::{get_global_string, StringId};
use rustc_hash::FxHashMap;

// 导入您的 MIR 类型
use litec_mir::{
    BasicBlockId, BinOp, FloatKind, IntKind, Literal, LocalId, MirFunction, Operand, Place, PlaceBase, Rvalue, Statement, SwitchTargets, Terminator, Ty, UnOp
};

pub struct CodeGen<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
    
    // 运行时状态
    local_vars: FxHashMap<LocalId, PointerValue<'ctx>>,
    basic_blocks: FxHashMap<BasicBlockId, inkwell::basic_block::BasicBlock<'ctx>>,
    local_types: FxHashMap<LocalId, Ty>,
    
    // 外部函数声明
    printf_fn: Option<FunctionValue<'ctx>>,
}

impl<'ctx> CodeGen<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();
        
        Self {
            context,
            module,
            builder,
            local_vars: FxHashMap::default(),
            basic_blocks: FxHashMap::default(),
            local_types: FxHashMap::default(),
            printf_fn: None,
        }
    }
    
    // 声明 printf 函数（使用不透明指针）
    fn declare_printf(&mut self) -> FunctionValue<'ctx> {
        if let Some(printf) = self.printf_fn {
            return printf;
        }
        
        let i32_type = self.context.i32_type();
        // 使用默认地址空间 (AddressSpace(0))
        let ptr_type = self.context.ptr_type(AddressSpace::default());
        
        // printf 函数签名：i32 (ptr, ...)
        let printf_type = i32_type.fn_type(&[ptr_type.into()], true);
        let printf_fn = self.module.add_function("printf", printf_type, None);
        self.printf_fn = Some(printf_fn);
        
        printf_fn
    }
    
    // 或者使用更清晰的方式
    fn declare_printf_clear(&mut self) -> FunctionValue<'ctx> {
        let i32_type = self.context.i32_type();
        
        // 多种创建 AddressSpace 的方式：
        let address_space = AddressSpace::from(0);
        
        let ptr_type = self.context.ptr_type(address_space);
        let printf_type = i32_type.fn_type(&[ptr_type.into()], true);
        let printf_fn = self.module.add_function("printf", printf_type, None);
        
        printf_fn
    }
    
    // 编译字符串字面量
    fn compile_string_literal(&self, content: &str, name: &str) -> PointerValue<'ctx> {
        // build_global_string_ptr 内部会处理地址空间
        let global_string = self.builder.build_global_string_ptr(content, name);
        global_string.expect("Failed to create global value").as_pointer_value()
    }
    
    // 为局部变量分配内存
    fn allocate_local(&mut self, local_id: LocalId, ty: &Ty) -> Option<()> {
        // 跳过 void 类型的局部变量分配
        if matches!(ty, Ty::Unit | Ty::Never) {
            return None;
        }
        
        let llvm_type = self.get_value_type(ty.clone())?;
        
        // build_alloca 返回的指针已经在正确的地址空间中
        let alloca = self.builder.build_alloca(llvm_type, &format!("local_{}", local_id.0)).expect("alloc error");
        self.local_vars.insert(local_id, alloca);
        
        Some(())
    }

     // 专门处理函数返回类型
    fn get_return_type(&self, ty: &Ty) -> inkwell::types::FunctionType<'ctx> {
        match ty {
            Ty::Unit | Ty::Never => {
                // void 类型需要特殊处理
                self.context.void_type().fn_type(&[], false)
            }
            _ => {
                if let Some(value_type) = self.get_value_type(ty.clone()) {
                    value_type.fn_type(&[], false)
                } else {
                    // 回退到 void
                    self.context.void_type().fn_type(&[], false)
                }
            }
        }
    }
    
    // 检查类型是否可以作为变量类型
    fn is_valid_variable_type(&self, ty: &Ty) -> bool {
        !matches!(ty, Ty::Unit | Ty::Never)
    }

    fn get_value_type(&self, ty: Ty) -> Option<BasicTypeEnum<'ctx>> {
        match ty {
            // 整数类型
            Ty::Int(int_kind) => Some(self.convert_int_type(int_kind).into()),
            
            // 浮点类型
            Ty::Float(float_kind) => Some(self.convert_float_type(float_kind).into()),
            
            // 布尔类型
            Ty::Bool => Some(self.context.bool_type().into()),
            
            // 未知类型 - 使用默认的 i8
            Ty::Unknown => Some(self.context.i8_type().into()),

            _ => None
        }
    }

    // 获取平台相关的指针大小
    fn get_pointer_size(&self) -> u32 {
        // 可以通过编译时配置或运行时检测
        #[cfg(target_pointer_width = "64")]
        return 64;
        
        #[cfg(target_pointer_width = "32")]
        return 32;
        
        #[cfg(target_pointer_width = "16")]
        return 16;
    }
    
    // 根据平台获取 isize/usize 类型
    fn get_platform_int_type(&self) -> inkwell::types::IntType<'ctx> {
        match self.get_pointer_size() {
            64 => self.context.i64_type(),
            32 => self.context.i32_type(),
            16 => self.context.i16_type(),
            _ => self.context.i64_type(), // 默认
        }
    }
    
    // 处理整数类型转换
    fn convert_int_type(&self, int_kind: IntKind) -> inkwell::types::IntType<'ctx> {
        match int_kind {
            // 有符号整数
            IntKind::I8 => self.context.i8_type(),
            IntKind::I16 => self.context.i16_type(),
            IntKind::I32 => self.context.i32_type(),
            IntKind::I64 => self.context.i64_type(),
            IntKind::I128 => self.context.i128_type(),
            IntKind::Isize => self.get_platform_int_type(),
            
            // 无符号整数 - LLVM 中实际上没有无符号类型的概念
            // 符号性在操作中体现，类型本身相同
            IntKind::U8 => self.context.i8_type(),
            IntKind::U16 => self.context.i16_type(),
            IntKind::U32 => self.context.i32_type(),
            IntKind::U64 => self.context.i64_type(),
            IntKind::U128 => self.context.i128_type(),
            IntKind::Usize => self.get_platform_int_type(), // 假设 64 位平台
        }
    }
    
    // 处理浮点类型转换
    fn convert_float_type(&self, float_kind: FloatKind) -> inkwell::types::FloatType<'ctx> {
        match float_kind {
            FloatKind::F32 => self.context.f32_type(),
            FloatKind::F64 => self.context.f64_type(),
        }
    }
}

impl<'ctx> CodeGen<'ctx> {
    // 编译操作数
    pub fn compile_operand(&mut self, operand: &Operand) -> Option<BasicValueEnum<'ctx>> {
        match operand {
            Operand::Literal(literal) => self.compile_literal(literal),
            Operand::Local(local_id) => self.compile_local(*local_id),
            Operand::Static(def_id) => {
                eprintln!("Warning: Static variables not yet implemented: {:?}", def_id);
                None
            }
        }
    }
    
    // 编译字面量
    fn compile_literal(&self, literal: &Literal) -> Option<BasicValueEnum<'ctx>> {
        match literal {
            Literal::I8(val) => Some(self.context.i8_type().const_int(*val as u64, false).into()),
            Literal::I16(val) => Some(self.context.i16_type().const_int(*val as u64, false).into()),
            Literal::I32(val) => Some(self.context.i32_type().const_int(*val as u64, false).into()),
            Literal::I64(val) => Some(self.context.i64_type().const_int(*val as u64, false).into()),
            Literal::I128(val) => {
                let high = ((*val as u128) >> 64) as u64;
                let low = (*val as u128) as u64;
                Some(self.context.i128_type().const_int_arbitrary_precision(&[low, high]).into())
            }
            Literal::U8(val) => Some(self.context.i8_type().const_int(*val as u64, false).into()),
            Literal::U16(val) => Some(self.context.i16_type().const_int(*val as u64, false).into()),
            Literal::U32(val) => Some(self.context.i32_type().const_int(*val as u64, false).into()),
            Literal::U64(val) => Some(self.context.i64_type().const_int(*val, false).into()),
            Literal::U128(val) => {
                let high = (*val >> 64) as u64;
                let low = *val as u64;
                Some(self.context.i128_type().const_int_arbitrary_precision(&[low, high]).into())
            }
            Literal::F32(val) => Some(self.context.f32_type().const_float(*val as f64).into()),
            Literal::F64(val) => Some(self.context.f64_type().const_float(*val).into()),
            Literal::Bool(val) => Some(self.context.bool_type().const_int(*val as u64, false).into()),
            Literal::Unit => None, // Unit 没有值
            Literal::Never => None, // Never 没有值
            Literal::Str(string_id) => {
                let content = get_global_string(*string_id).unwrap();
                let global_str = self.builder.build_global_string_ptr(&content, &format!("str_{}", string_id.0))
                    .expect("Failed to create global string");
                Some(global_str.as_pointer_value().into())
            }
            Literal::Char(ch) => Some(self.context.i8_type().const_int(*ch as u64, false).into()),
            Literal::Isize(val) => Some(self.get_platform_int_type().const_int(*val as u64, false).into()),
            Literal::Usize(val) => Some(self.get_platform_int_type().const_int(*val as u64, false).into()),
        }
    }

    // 编译终结符
    fn compile_terminator(&mut self, terminator: &Terminator, return_ty: &Ty) -> Option<()> {
        match terminator {
            Terminator::Goto { target, span: _ } => {
                let target_bb = self.basic_blocks.get(target)?;
                self.builder.build_unconditional_branch(*target_bb);
                Some(())
            }
            Terminator::Return { value, span: _ } => {
                match return_ty {
                    Ty::Unit | Ty::Never => {
                        // void 返回
                        self.builder.build_return(None);
                    }
                    _ => {
                        // 有值返回
                        if let Some(return_value) = self.compile_operand(value) {
                            self.builder.build_return(Some(&return_value));
                        } else {
                            eprintln!("Error: Failed to compile return value");
                            return None;
                        }
                    }
                }
                Some(())
            }
            Terminator::Unreachable { span: _ } => {
                self.builder.build_unreachable();
                Some(())
            }
            Terminator::Switch { discr, targets, span: _ } => {
                self.compile_switch(discr, targets)
            }
        }
    }
    
    // 编译 switch 语句
    fn compile_switch(&mut self, discr: &Operand, targets: &SwitchTargets) -> Option<()> {
        let discr_val = self.compile_operand(discr)?.into_int_value();
        let otherwise_bb = *self.basic_blocks.get(&targets.otherwise)?;
        
        // Prepare the cases as a vector of tuples (case_value, target_basic_block)
        let cases: Vec<_> = targets.values.iter()
            .zip(&targets.targets) // Combine values and targets
            .filter_map(|(&value, &bb_id)| {
                // Look up the LLVM BasicBlock for each BasicBlockId
                self.basic_blocks.get(&bb_id).map(|&bb| {
                    let case_val = discr_val.get_type().const_int(value.try_into().unwrap(), false);
                    (case_val, bb)
                })
            })
            .collect();
        
        // Build the switch with all cases provided at once
        let switch_result = self.builder.build_switch(
            discr_val,
            otherwise_bb,
            &cases, // Pass the prepared cases slice
        );
    
        // Handle the Result, typically you'd use `?` or `expect` here
        match switch_result {
            Ok(_) => Some(()),
            Err(e) => {
                eprintln!("Failed to build switch instruction: {:?}", e);
                None
            }
        }
    }
    
    // 编译局部变量访问
    fn compile_local(&mut self, local_id: LocalId) -> Option<BasicValueEnum<'ctx>> {
        let ptr = self.local_vars.get(&local_id)?;
        let ty = self.local_types.get(&local_id)?;
        
        if let Some(value_type) = self.get_value_type(ty.clone()) {
            Some(self.builder.build_load(value_type, *ptr, &format!("load_{}", local_id.0)).unwrap().into())
        } else {
            None
        }
    }

    /// 编译 MIR 语句
    fn compile_statement(&mut self, statement: &Statement) -> Option<()> {
        match statement {
            Statement::Assign { dest, rvalue, span: _ } => {
                // 1. 编译右侧表达式，获取其值
                let rvalue_val = self.compile_rvalue(rvalue)?;
                
                // 2. 获取目标局部变量的内存指针
                if let Some(dest_ptr) = self.local_vars.get(dest) {
                    // 3. 生成 LLVM store 指令，将右值存入左值位置
                    self.builder.build_store(*dest_ptr, rvalue_val);
                    Some(())
                } else {
                    eprintln!("Error: Local variable {:?} not found", dest);
                    None
                }
            }
            // 未来可以扩展其他语句类型，例如：
            // Statement::StorageLive(local) => { ... }
            // Statement::StorageDead(local) => { ... }
        }
    }

    fn compile_binop_enhanced(
        &self, 
        op: BinOp, 
        lhs: BasicValueEnum<'ctx>, 
        rhs: BasicValueEnum<'ctx>,
        lhs_ty: &Ty,
        rhs_ty: &Ty
    ) -> Option<BasicValueEnum<'ctx>> {
        match (lhs, rhs) {
            (BasicValueEnum::IntValue(l), BasicValueEnum::IntValue(r)) => {
                let is_signed = self.is_signed_int(lhs_ty) || self.is_signed_int(rhs_ty);

                Some(match op {
                    BinOp::Add => {
                        // LLVM 的整数加法本身不区分有符号和无符号
                        // 使用 build_int_add 即可
                        self.builder.build_int_add(l, r, "add").unwrap().into()
                    }
                    BinOp::Sub => {
                        // LLVM 的整数减法本身不区分有符号和无符号  
                        // 使用 build_int_sub 即可
                        self.builder.build_int_sub(l, r, "sub").unwrap().into()
                    }
                    BinOp::Mul => {
                        // LLVM 的整数乘法本身不区分有符号和无符号
                        // 使用 build_int_mul 即可
                        self.builder.build_int_mul(l, r, "mul").unwrap().into()
                    }
                    BinOp::Div => {
                        if is_signed {
                            self.builder.build_int_signed_div(l, r, "sdiv").unwrap().into()
                        } else {
                            self.builder.build_int_unsigned_div(l, r, "udiv").unwrap().into()
                        }
                    }
                    BinOp::Rem => {
                        if is_signed {
                            self.builder.build_int_signed_rem(l, r, "srem").unwrap().into()
                        } else {
                            self.builder.build_int_unsigned_rem(l, r, "urem").unwrap().into()
                        }
                    }
                    // 比较运算需要根据符号性选择正确的谓词
                    BinOp::Lt => {
                        let predicate = if is_signed { IntPredicate::SLT } else { IntPredicate::ULT };
                        self.builder.build_int_compare(predicate, l, r, "lt").unwrap().into()
                    }
                    BinOp::Le => {
                        let predicate = if is_signed { IntPredicate::SLE } else { IntPredicate::ULE };
                        self.builder.build_int_compare(predicate, l, r, "le").unwrap().into()
                    }
                    BinOp::Gt => {
                        let predicate = if is_signed { IntPredicate::SGT } else { IntPredicate::UGT };
                        self.builder.build_int_compare(predicate, l, r, "gt").unwrap().into()
                    }
                    BinOp::Ge => {
                        let predicate = if is_signed { IntPredicate::SGE } else { IntPredicate::UGE };
                        self.builder.build_int_compare(predicate, l, r, "ge").unwrap().into()
                    }
                    // 这些运算与符号性无关
                    BinOp::Eq => self.builder.build_int_compare(IntPredicate::EQ, l, r, "eq").unwrap().into(),
                    BinOp::Ne => self.builder.build_int_compare(IntPredicate::NE, l, r, "ne").unwrap().into(),
                    BinOp::And => self.builder.build_and(l, r, "and").unwrap().into(),
                    BinOp::Or => self.builder.build_or(l, r, "or").unwrap().into(),
                })
            }
            // 浮点数运算
            (BasicValueEnum::FloatValue(l), BasicValueEnum::FloatValue(r)) => {
                Some(match op {
                    BinOp::Add => self.builder.build_float_add(l, r, "fadd").unwrap().into(),
                    BinOp::Sub => self.builder.build_float_sub(l, r, "fsub").unwrap().into(),
                    BinOp::Mul => self.builder.build_float_mul(l, r, "fmul").unwrap().into(),
                    BinOp::Div => self.builder.build_float_div(l, r, "fdiv").unwrap().into(),
                    BinOp::Rem => self.builder.build_float_rem(l, r, "frem").unwrap().into(),
                    BinOp::Eq => self.builder.build_float_compare(FloatPredicate::OEQ, l, r, "feq").unwrap().into(),
                    BinOp::Ne => self.builder.build_float_compare(FloatPredicate::ONE, l, r, "fne").unwrap().into(),
                    BinOp::Lt => self.builder.build_float_compare(FloatPredicate::OLT, l, r, "flt").unwrap().into(),
                    BinOp::Le => self.builder.build_float_compare(FloatPredicate::OLE, l, r, "fle").unwrap().into(),
                    BinOp::Gt => self.builder.build_float_compare(FloatPredicate::OGT, l, r, "fgt").unwrap().into(),
                    BinOp::Ge => self.builder.build_float_compare(FloatPredicate::OGE, l, r, "fge").unwrap().into(),
                    _ => {
                        eprintln!("Warning: Operation {:?} not supported for floats", op);
                        return None;
                    }
                })
            }
            // 其他情况回退到基础版本
            _ => {
                eprintln!("Warning: Unsupported operand types for binary operation");
                return None;
            }
        }
    }

    // 辅助函数：判断整数类型是否有符号
    fn is_signed_int(&self, ty: &Ty) -> bool {
        matches!(ty, 
            Ty::Int(IntKind::I8 | IntKind::I16 | IntKind::I32 | 
                    IntKind::I64 | IntKind::I128 | IntKind::Isize)
        )
    }

    fn get_operand_type(&self, operand: &Operand) -> Option<Ty> {
        match operand {
            Operand::Literal(literal) => Some(self.get_literal_type(literal)),
            Operand::Local(local_id) => self.local_types.get(local_id).cloned(),
            Operand::Static(def_id) => {
                eprintln!("Warning: Static operand type lookup not yet implemented for {:?}", def_id);
                // 处理静态变量类型
                None // 需要根据你的实现来完善
            }
        }
    }

    fn get_literal_type(&self, literal: &Literal) -> Ty {
        match literal {
            Literal::I8(_) => Ty::Int(IntKind::I8),
            Literal::I16(_) => Ty::Int(IntKind::I16),
            Literal::I32(_) => Ty::Int(IntKind::I32),
            Literal::I64(_) => Ty::Int(IntKind::I64),
            Literal::I128(_) => Ty::Int(IntKind::I128),
            Literal::U8(_) => Ty::Int(IntKind::U8),
            Literal::U16(_) => Ty::Int(IntKind::U16),
            Literal::U32(_) => Ty::Int(IntKind::U32),
            Literal::U64(_) => Ty::Int(IntKind::U64),
            Literal::U128(_) => Ty::Int(IntKind::U128),
            Literal::F32(_) => Ty::Float(FloatKind::F32),
            Literal::F64(_) => Ty::Float(FloatKind::F64),
            Literal::Bool(_) => Ty::Bool,
            Literal::Unit => Ty::Unit,
            Literal::Never => Ty::Never,
            Literal::Str(_) => Ty::Str,
            Literal::Char(_) => {
                // 字符类型 - 通常用 i8 或 i32 表示
                Ty::Int(IntKind::I32)
            }
            Literal::Isize(_) => Ty::Int(IntKind::Isize),
            Literal::Usize(_) => Ty::Int(IntKind::Usize),
        }
    }

    fn compile_unop_enhanced(
        &self, 
        op: UnOp, 
        operand: BasicValueEnum<'ctx>,
        operand_ty: &Ty
    ) -> Option<BasicValueEnum<'ctx>> {
        match op {
            UnOp::Not => {
                // 逻辑非运算只支持整数和布尔类型
                if !self.is_valid_for_not(operand_ty) {
                    eprintln!("Warning: Not operation not supported for type {:?}", operand_ty);
                    return None;
                }
                
                match operand {
                    BasicValueEnum::IntValue(val) => {
                        Some(self.builder.build_not(val, "not").unwrap().into())
                    }
                    _ => {
                        eprintln!("Warning: Unexpected operand type for Not operation");
                        None
                    }
                }
            }
            UnOp::Neg => {
                // 算术取负运算只支持数值类型
                if !self.is_valid_for_neg(operand_ty) {
                    eprintln!("Warning: Negation not supported for type {:?}", operand_ty);
                    return None;
                }
                
                match operand {
                    BasicValueEnum::IntValue(val) => {
                        Some(self.builder.build_int_neg(val, "neg").unwrap().into())
                    }
                    BasicValueEnum::FloatValue(val) => {
                        Some(self.builder.build_float_neg(val, "fneg").unwrap().into())
                    }
                    _ => {
                        eprintln!("Warning: Unexpected operand type for Negation");
                        None
                    }
                }
            }
        }
    }
    
    // 辅助函数：检查类型是否支持逻辑非运算
    fn is_valid_for_not(&self, ty: &Ty) -> bool {
        matches!(ty, 
            Ty::Int(_) | Ty::Bool
        )
    }
    
    // 辅助函数：检查类型是否支持算术取负运算
    fn is_valid_for_neg(&self, ty: &Ty) -> bool {
        matches!(ty, 
            Ty::Int(_) | Ty::Float(_)
        )
    }

    /// 编译右侧值 (Rvalue)
    fn compile_rvalue(&mut self, rvalue: &Rvalue) -> Option<BasicValueEnum<'ctx>> {
        match rvalue {
            Rvalue::Use(operand) => {
                // 直接使用操作数的值
                self.compile_operand(operand)
            }
            Rvalue::Binary(op, lhs, rhs) => {
                // 编译二元操作
                let lhs_val = self.compile_operand(lhs)?;
                let rhs_val = self.compile_operand(rhs)?;
                self.compile_binop_enhanced(*op, lhs_val, rhs_val, &self.get_operand_type(lhs).expect("unknow operand"), &self.get_operand_type(rhs).expect("unknow operand"))
            }
            Rvalue::Unary(op, operand) => {
                // 编译一元操作
                let operand_val = self.compile_operand(operand)?;
                self.compile_unop_enhanced(*op, operand_val, &self.get_operand_type(operand).expect("unknow operand"))
            }
            Rvalue::Ref(_, place) => {
                // 取地址操作：获取地值（Place）的指针
                self.compile_place(place).map(|ptr| ptr.into())
            }
            // 注意：此处需要根据你的 MIR 定义，处理 Rvalue 的其他变体，例如：
            // Rvalue::CheckedBinary, Rvalue::Aggregate, Rvalue::Len 等。
            _ => {
                eprintln!("Warning: Rvalue variant {:?} not yet implemented", rvalue);
                None
            }
        }
    }

    /// 编译地值 (Place) 以获取指针
    fn compile_place(&mut self, place: &Place) -> Option<PointerValue<'ctx>> {
        // 这里是一个基础实现，仅处理基位置，未处理投影（索引、字段等）
        match &place.base {
            PlaceBase::Local(local_id) => {
                // 从局部变量映射中获取已分配的内存指针
                self.local_vars.get(local_id).copied()
            }
            PlaceBase::Static(def_id) => {
                // 处理静态变量
                eprintln!("Warning: Static place not yet implemented: {:?}", def_id);
                None
            }
        }
        // 注意：一个完整的实现还需要处理 `place.projections`（例如 Deref, Field, Index）
        // 以计算最终的内存地址。
    }
    
    // 主编译函数
    pub fn compile_function(&mut self, mir_func: &MirFunction) -> Option<FunctionValue<'ctx>> {
        // 清理状态
        self.local_vars.clear();
        self.basic_blocks.clear();
        self.local_types.clear();
        
        // 1. 准备函数类型
        let return_ty = if let Some(first_local) = mir_func.locals.first() {
            &first_local.ty
        } else {
            &Ty::Unit
        };
        
        let fn_type = self.get_return_type(return_ty);
        let function = self.module.add_function(&format!("fn_{}_{:?}", mir_func.def_id.index, mir_func.def_id.kind), fn_type, None);
        
        // 2. 创建基本块
        for bb in &mir_func.basic_blocks {
            let llvm_bb = self.context.append_basic_block(function, &format!("bb_{}", bb.id.0));
            self.basic_blocks.insert(bb.id, llvm_bb);
        }
        
        // 3. 设置入口块
        let entry_bb = self.basic_blocks.get(&BasicBlockId(0))?;
        self.builder.position_at_end(*entry_bb);
        
        // 4. 分配局部变量
        for (i, local) in mir_func.locals.iter().enumerate() {
            let local_id = LocalId(i);
            self.local_types.insert(local_id, local.ty.clone());
            
            if self.is_valid_variable_type(&local.ty) {
                if let Some(value_type) = self.get_value_type(local.ty.clone()) {
                    let alloca = self.builder.build_alloca(value_type, &format!("local_{}", i))
                        .expect("Failed to allocate local variable");
                    self.local_vars.insert(local_id, alloca);
                }
            }
        }
        
        // 5. 编译所有基本块
        for bb in &mir_func.basic_blocks {
            if let Some(llvm_bb) = self.basic_blocks.get(&bb.id) {
                self.builder.position_at_end(*llvm_bb);
                
                // 编译语句
                for statement in &bb.statements {
                    self.compile_statement(statement);
                }
                
                // 编译终结符
                if let Some(terminator) = &bb.terminator {
                    self.compile_terminator(terminator, return_ty);
                }
            }
        }
        
        // 调用 verify 方法，并传入 bool 参数
        let is_valid = function.verify(true); // 传入 true，验证失败时打印信息

        if !is_valid {
            eprintln!("Function verification failed for: {:?}", mir_func.def_id);
            // 可以考虑在这里处理错误，例如返回 None 或记录日志
            return None;
        }
        
        Some(function)
    }
}

impl<'ctx> CodeGen<'ctx> {
    /// 直接生成二进制代码并返回字节向量
    pub fn compile_to_binary(&self) -> Result<Vec<u8>, String> {
        // 1. 初始化 LLVM 目标平台
        Self::initialize_targets();
        
        // 2. 获取当前主机目标三元组
        let target_triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&target_triple)
            .map_err(|e| format!("Failed to get target: {}", e))?;
        
        // 3. 创建目标机器
        let cpu = TargetMachine::get_host_cpu_name().to_string();
        let features = TargetMachine::get_host_cpu_features().to_string();
        
        let target_machine = target.create_target_machine(
            &target_triple,
            &cpu,
            &features,
            OptimizationLevel::Default,
            RelocMode::Default,
            CodeModel::Default,
        ).ok_or("Failed to create target machine")?;
        
        // 4. 将模块转换为内存中的二进制
        let memory_buffer = target_machine
            .write_to_memory_buffer(&self.module, FileType::Object)
            .map_err(|e| format!("Failed to write object file: {}", e))?;
        
        // 5. 提取二进制数据
        let binary_data = unsafe {
            std::slice::from_raw_parts(
                memory_buffer.as_slice().as_ptr() as *const u8,
                memory_buffer.as_slice().len()
            ).to_vec()
        };
        
        Ok(binary_data)
    }
    
    /// 生成可执行文件
    pub fn compile_to_executable(&self, output_path: &Path) -> Result<(), String> {
        Self::initialize_targets();
        
        let target_triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&target_triple)
            .map_err(|e| format!("Failed to get target: {}", e))?;
        
        let cpu = TargetMachine::get_host_cpu_name().to_string();
        let features = TargetMachine::get_host_cpu_features().to_string();
        
        let target_machine = target.create_target_machine(
            &target_triple,
            &cpu,
            &features,
            OptimizationLevel::Default,
            RelocMode::Default,
            CodeModel::Default,
        ).ok_or("Failed to create target machine")?;
        
        target_machine
            .write_to_file(&self.module, FileType::Object, output_path)
            .map_err(|e| format!("Failed to write executable: {}", e))?;
        
        Ok(())
    }
    
    /// 初始化 LLVM 目标平台
    fn initialize_targets() {
        Target::initialize_all(&InitializationConfig::default());
    }
    
    pub fn compile_to_assembly(&self) -> Result<String, String> {
        Self::initialize_targets();
        
        let target_triple = TargetMachine::get_default_triple();
        let target = Target::from_triple(&target_triple)
            .map_err(|e| format!("Failed to get target: {}", e))?;
        
        let cpu = TargetMachine::get_host_cpu_name().to_string();
        let features = TargetMachine::get_host_cpu_features().to_string();
        
        let target_machine = target.create_target_machine(
            &target_triple,
            &cpu,
            &features,
            OptimizationLevel::Default,
            RelocMode::Default,
            CodeModel::Default,
        ).ok_or("Failed to create target machine")?;

        // 使用内存缓冲区获取汇编代码
        let memory_buffer = target_machine
            .write_to_memory_buffer(&self.module, FileType::Assembly)
            .map_err(|e| format!("Failed to generate assembly: {}", e))?;

        // 转换为字符串
        let assembly = String::from_utf8(memory_buffer.as_slice().to_vec())
            .map_err(|e| format!("Failed to convert assembly to UTF-8 string: {}", e))?;

        Ok(assembly)
    }
}

pub fn codegen(mir: Vec<MirFunction>, name: &str) -> Result<Vec<u8>, String> {
    let context = Context::create();
    let mut codegen = CodeGen::new(&context, name);

    println!("🔧 开始代码生成，共 {} 个函数", mir.len());

    for (i, function) in mir.iter().enumerate() {
        println!("📋 编译第 {} 个函数: {:?}", i + 1, function.def_id);
        
        match codegen.compile_function(function) {
            Some(_) => println!("✅ 函数编译成功"),
            None => {
                eprintln!("❌ 函数编译失败: {:?}", function.def_id);
                // 打印更多调试信息
                eprintln!("   基本块数量: {}", function.basic_blocks.len());
                eprintln!("   局部变量数量: {}", function.locals.len());
                return Err(format!("无法编译函数: {:?}", function.def_id).into());
            }
        }
    }

    // 验证模块
    println!("🔍 验证LLVM模块...");
    if let Err(e) = codegen.module.verify() {
        eprintln!("❌ 模块验证失败: {}", e);
        // 打印模块内容用于调试
        let ir = codegen.module.print_to_string();
        eprintln!("生成的LLVM IR:\n{}", ir);
        return Err(format!("模块验证失败: {}", e).into());
    }

    println!("✅ 模块验证成功");

    // 生成二进制
    codegen.compile_to_binary()
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    use litec_mir::{BasicBlock, LocalDecl, Terminator};
    use litec_span::StringId;
    use litec_typed_hir::{def_id::DefId, DefKind};

    // 辅助函数：创建测试用的 DefId
    fn create_test_def_id() -> DefId {
        DefId {
            index: 0,
            kind: DefKind::Function,
        }
    }

    // 辅助函数：创建测试用的 StringId
    fn create_test_string_id() -> StringId {
        StringId(0)
    }

    // 辅助函数：创建测试用的 Span
    fn create_test_span() -> litec_span::Span {
        litec_span::Span::default()
    }

    // 测试基础架构
    #[test]
    fn test_codegen_initialization() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test_module");
        
        assert!(codegen.module.get_name().to_str().unwrap().contains("test_module"));
        assert!(codegen.local_vars.is_empty());
        assert!(codegen.basic_blocks.is_empty());
    }

    // 测试类型转换
    #[test]
    fn test_type_conversion() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test_types");

        // 测试整数类型转换
        let i32_ty = codegen.get_value_type(Ty::Int(IntKind::I32));
        assert!(i32_ty.is_some());
        
        let f64_ty = codegen.get_value_type(Ty::Float(FloatKind::F64));
        assert!(f64_ty.is_some());
        
        let bool_ty = codegen.get_value_type(Ty::Bool);
        assert!(bool_ty.is_some());
        
        // 测试 void 类型
        let unit_ty = codegen.get_value_type(Ty::Unit);
        assert!(unit_ty.is_none());
        
        let never_ty = codegen.get_value_type(Ty::Never);
        assert!(never_ty.is_none());
    }

    // 测试字面量编译
    #[test]
    fn test_literal_compilation() {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "test_literals");

        // 测试整数字面量
        let i32_literal = Literal::I32(42);
        let compiled_i32 = codegen.compile_literal(&i32_literal);
        assert!(compiled_i32.is_some());

        // 测试浮点数字面量
        let f64_literal = Literal::F64(3.14);
        let compiled_f64 = codegen.compile_literal(&f64_literal);
        assert!(compiled_f64.is_some());

        // 测试布尔字面量
        let bool_literal = Literal::Bool(true);
        let compiled_bool = codegen.compile_literal(&bool_literal);
        assert!(compiled_bool.is_some());

        // 测试 Unit 字面量
        let unit_literal = Literal::Unit;
        let compiled_unit = codegen.compile_literal(&unit_literal);
        assert!(compiled_unit.is_none());
    }

    // 测试操作数编译
    #[test]
    fn test_operand_compilation() {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "test_operands");

        // 测试字面量操作数
        let literal_operand = Operand::Literal(Literal::I32(100));
        let compiled_literal = codegen.compile_operand(&literal_operand);
        assert!(compiled_literal.is_some());

        // 测试局部变量操作数（需要先分配变量）
        let local_operand = Operand::Local(LocalId(0));
        let compiled_local = codegen.compile_operand(&local_operand);
        assert!(compiled_local.is_none()); // 应该失败，因为没有分配该局部变量
    }

    // 测试一元运算编译
    #[test]
    fn test_unary_operations() {
        let context = Context::create();
        let module = context.create_module("test_unary");
        let builder = context.create_builder();
        let codegen = CodeGen {
            context: &context,
            module,
            builder,
            local_vars: FxHashMap::default(),
            basic_blocks: FxHashMap::default(),
            local_types: FxHashMap::default(),
            printf_fn: None,
        };

        // 创建一个测试函数和基本块
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = codegen.module.add_function("test_unary_func", fn_type, None);
        let entry_block = context.append_basic_block(function, "entry");

        // 设置构建器位置
        codegen.builder.position_at_end(entry_block);

        // 测试整数取负
        let int_val = codegen.context.i32_type().const_int(42, false).into();
        let neg_result = codegen.compile_unop_enhanced(
            UnOp::Neg, 
            int_val, 
            &Ty::Int(IntKind::I32)
        );
        assert!(neg_result.is_some());

        // 测试浮点数取负
        let float_val = codegen.context.f64_type().const_float(3.14).into();
        let fneg_result = codegen.compile_unop_enhanced(
            UnOp::Neg, 
            float_val, 
            &Ty::Float(FloatKind::F64)
        );
        assert!(fneg_result.is_some());

        // 测试逻辑非
        let bool_val = codegen.context.bool_type().const_int(1, false).into();
        let not_result = codegen.compile_unop_enhanced(
            UnOp::Not, 
            bool_val, 
            &Ty::Bool
        );
        assert!(not_result.is_some());
    }

    // 测试二元运算编译
    #[test]
    fn test_binary_operations() {
            let context = Context::create();
        let module = context.create_module("test_binary");
        let builder = context.create_builder();
        let codegen = CodeGen {
            context: &context,
            module,
            builder,
            local_vars: FxHashMap::default(),
            basic_blocks: FxHashMap::default(),
            local_types: FxHashMap::default(),
            printf_fn: None,
        };

        // 创建一个测试函数和基本块
        let fn_type = context.i32_type().fn_type(&[], false);
        let function = codegen.module.add_function("test_binary_func", fn_type, None);
        let entry_block = context.append_basic_block(function, "entry");

        // 设置构建器位置
        codegen.builder.position_at_end(entry_block);

        // 测试整数加法
        let lhs = codegen.context.i32_type().const_int(10, false).into();
        let rhs = codegen.context.i32_type().const_int(20, false).into();
        let add_result = codegen.compile_binop_enhanced(
            BinOp::Add, 
            lhs, 
            rhs, 
            &Ty::Int(IntKind::I32), 
            &Ty::Int(IntKind::I32)
        );
        assert!(add_result.is_some());

        // 测试浮点数运算
        let flhs = codegen.context.f32_type().const_float(1.0).into();
        let frhs = codegen.context.f32_type().const_float(2.0).into();
        let fadd_result = codegen.compile_binop_enhanced(
            BinOp::Add, 
            flhs, 
            frhs, 
            &Ty::Float(FloatKind::F32), 
            &Ty::Float(FloatKind::F32)
        );
        assert!(fadd_result.is_some());
    }

    // 测试简单的函数编译
    #[test]
    fn test_simple_function_compilation() {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "test_simple_func");

        // 创建简单的 MIR 函数
        let mir_func = MirFunction {
            def_id: create_test_def_id(),
            name: create_test_string_id(),
            locals: vec![
                LocalDecl {
                    ty: Ty::Int(IntKind::I32),
                    name: Some(create_test_string_id()),
                    span: create_test_span(),
                }
            ],
            basic_blocks: vec![
                BasicBlock {
                    id: BasicBlockId(0),
                    statements: vec![],
                    terminator: Some(Terminator::Return {
                        value: Operand::Literal(Literal::I32(42)),
                        span: create_test_span(),
                    }),
                }
            ],
            span: create_test_span(),
        };

        // 编译函数
        let result = codegen.compile_function(&mir_func);
        assert!(result.is_some());

        // 验证生成的模块
        let module = &codegen.module;
        assert!(!module.print_to_string().to_string().is_empty());
    }

    // 测试控制流编译
    #[test]
    fn test_control_flow_compilation() {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "test_control_flow");

        // 创建包含多个基本块的 MIR 函数
        let mir_func = MirFunction {
            def_id: create_test_def_id(),
            name: create_test_string_id(),
            locals: vec![
                LocalDecl {
                    ty: Ty::Int(IntKind::I32),
                    name: Some(create_test_string_id()),
                    span: create_test_span(),
                },
                LocalDecl {
                    ty: Ty::Bool,
                    name: Some(create_test_string_id()),
                    span: create_test_span(),
                }
            ],
            basic_blocks: vec![
                BasicBlock {
                    id: BasicBlockId(0),
                    statements: vec![
                        Statement::Assign {
                            dest: LocalId(1),
                            rvalue: Rvalue::Use(Operand::Literal(Literal::Bool(true))),
                            span: create_test_span(),
                        }
                    ],
                    terminator: Some(Terminator::Goto {
                        target: BasicBlockId(1),
                        span: create_test_span(),
                    }),
                },
                BasicBlock {
                    id: BasicBlockId(1),
                    statements: vec![],
                    terminator: Some(Terminator::Return {
                        value: Operand::Literal(Literal::I32(0)),
                        span: create_test_span(),
                    }),
                }
            ],
            span: create_test_span(),
        };

        let result = codegen.compile_function(&mir_func);
        assert!(result.is_some());
    }

    // 测试类型检查辅助函数
    #[test]
    fn test_type_helpers() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test_helpers");

        // 测试有符号整数判断
        assert!(codegen.is_signed_int(&Ty::Int(IntKind::I32)));
        assert!(codegen.is_signed_int(&Ty::Int(IntKind::Isize)));
        assert!(!codegen.is_signed_int(&Ty::Int(IntKind::U32)));
        assert!(!codegen.is_signed_int(&Ty::Bool));

        // 测试运算有效性检查
        assert!(codegen.is_valid_for_not(&Ty::Bool));
        assert!(codegen.is_valid_for_not(&Ty::Int(IntKind::I32)));
        assert!(!codegen.is_valid_for_not(&Ty::Float(FloatKind::F32)));

        assert!(codegen.is_valid_for_neg(&Ty::Int(IntKind::I32)));
        assert!(codegen.is_valid_for_neg(&Ty::Float(FloatKind::F64)));
        assert!(!codegen.is_valid_for_neg(&Ty::Bool));
    }

    // 测试操作数类型推断
    #[test]
    fn test_operand_type_inference() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test_type_inference");

        // 测试字面量类型推断
        let i32_literal = Operand::Literal(Literal::I32(42));
        let i32_ty = codegen.get_operand_type(&i32_literal);
        assert_eq!(i32_ty, Some(Ty::Int(IntKind::I32)));

        let f64_literal = Operand::Literal(Literal::F64(3.14));
        let f64_ty = codegen.get_operand_type(&f64_literal);
        assert_eq!(f64_ty, Some(Ty::Float(FloatKind::F64)));

        let bool_literal = Operand::Literal(Literal::Bool(true));
        let bool_ty = codegen.get_operand_type(&bool_literal);
        assert_eq!(bool_ty, Some(Ty::Bool));
    }

    // 测试错误情况处理
    #[test]
    fn test_error_handling() {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "test_errors");

        // 测试不支持的 Rvalue 变体
        let unsupported_rvalue = Rvalue::Len(Place {
            base: PlaceBase::Local(LocalId(0)),
            projections: vec![],
        });
        
        let result = codegen.compile_rvalue(&unsupported_rvalue);
        assert!(result.is_none());

        // 测试不存在的局部变量访问
        let invalid_local = Operand::Local(LocalId(999));
        let result = codegen.compile_operand(&invalid_local);
        assert!(result.is_none());
    }

    // 测试平台相关的整数类型
    #[test]
    fn test_platform_specific_types() {
        let context = Context::create();
        let codegen = CodeGen::new(&context, "test_platform");

        let isize_ty = codegen.convert_int_type(IntKind::Isize);
        let usize_ty = codegen.convert_int_type(IntKind::Usize);

        // 在 64 位平台上，isize 和 usize 应该是 64 位
        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(isize_ty.get_bit_width(), 64);
            assert_eq!(usize_ty.get_bit_width(), 64);
        }

        // 在 32 位平台上，isize 和 usize 应该是 32 位
        #[cfg(target_pointer_width = "32")]
        {
            assert_eq!(isize_ty.get_bit_width(), 32);
            assert_eq!(usize_ty.get_bit_width(), 32);
        }
    }

    // 集成测试：编译完整的计算函数
    #[test]
    fn test_integration_arithmetic_function() {
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, "test_integration");

        // 创建一个计算 (a + b) * 2 的函数
        let mir_func = MirFunction {
            def_id: create_test_def_id(),
            name: create_test_string_id(),
            locals: vec![
                LocalDecl { // return value
                    ty: Ty::Int(IntKind::I32),
                    name: Some(create_test_string_id()),
                    span: create_test_span(),
                },
                LocalDecl { // a
                    ty: Ty::Int(IntKind::I32),
                    name: Some(create_test_string_id()),
                    span: create_test_span(),
                },
                LocalDecl { // b  
                    ty: Ty::Int(IntKind::I32),
                    name: Some(create_test_string_id()),
                    span: create_test_span(),
                },
                LocalDecl { // temp result
                    ty: Ty::Int(IntKind::I32),
                    name: Some(create_test_string_id()),
                    span: create_test_span(),
                }
            ],
            basic_blocks: vec![
                BasicBlock {
                    id: BasicBlockId(0),
                    statements: vec![
                        // a = 10
                        Statement::Assign {
                            dest: LocalId(1),
                            rvalue: Rvalue::Use(Operand::Literal(Literal::I32(10))),
                            span: create_test_span(),
                        },
                        // b = 20
                        Statement::Assign {
                            dest: LocalId(2),
                            rvalue: Rvalue::Use(Operand::Literal(Literal::I32(20))),
                            span: create_test_span(),
                        },
                        // temp = a + b
                        Statement::Assign {
                            dest: LocalId(3),
                            rvalue: Rvalue::Binary(
                                BinOp::Add,
                                Operand::Local(LocalId(1)),
                                Operand::Local(LocalId(2)),
                            ),
                            span: create_test_span(),
                        },
                        // return temp * 2
                        Statement::Assign {
                            dest: LocalId(0),
                            rvalue: Rvalue::Binary(
                                BinOp::Mul,
                                Operand::Local(LocalId(3)),
                                Operand::Literal(Literal::I32(2)),
                            ),
                            span: create_test_span(),
                        },
                    ],
                    terminator: Some(Terminator::Return {
                        value: Operand::Local(LocalId(0)),
                        span: create_test_span(),
                    }),
                }
            ],
            span: create_test_span(),
        };

        let result = codegen.compile_function(&mir_func);
        assert!(result.is_some());

        // 输出生成的 IR 用于调试
        println!("Generated IR for arithmetic function:");
        println!("{}", codegen.module.print_to_string().to_string());
    }
}