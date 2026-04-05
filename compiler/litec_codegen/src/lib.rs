pub mod linker;

use inkwell::{
    self, AddressSpace, IntPredicate,
    builder::Builder,
    context::Context,
    module::{Linkage, Module},
    targets::{CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine},
    types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum},
    values::{
        BasicValue, BasicValueEnum, FloatValue, FunctionValue, IntValue, PointerValue, ValueKind,
    },
};
use litec_error::{Diagnostic, error};
use litec_hir::{AbiType, BinOp, FloatKind, LiteralIntKind, LiteralValue, UnOp};
use litec_mir::{
    self, AggregateKind, BasicBlock, Constant, ConstantKind, Local, LocalDecl, MirCrate, MirExtern,
    MirExternFunction, MirExternItem, MirFunction, MirItem, MirModule, MirStruct, MirUse, Operand,
    Place, PlaceElem, Rvalue, Statement, StatementKind, Terminator, TerminatorKind,
};
use litec_span::{Span, get_global_string};
use litec_typed_hir::{CastKind, builtins::BuiltinFunction, def_id::DefId, ty::Ty};
use rustc_hash::FxHashMap;
use std::path::Path;

use crate::linker::Linker;

#[derive(Clone, Copy)]
struct PointerInfo<'ctx> {
    ptr: PointerValue<'ctx>,
    pointee_ty: BasicTypeEnum<'ctx>,
    is_signed: bool,
}

#[derive(Clone, Debug)]
struct TypeInfo<'ctx> {
    llvm_type: BasicTypeEnum<'ctx>,
    ty: Ty,
}

pub struct CodeGen<'ctx> {
    context: &'ctx Context,
    module: Module<'ctx>,
    builder: Builder<'ctx>,
    mir_crate: MirCrate,
    /// 函数定义映射
    value_map: FxHashMap<DefId, FunctionValue<'ctx>>,
    /// 当前函数的局部变量映射（参数 + 局部变量）
    locals: FxHashMap<Local, PointerInfo<'ctx>>,
    /// 当前正在处理的函数
    current_function: Option<FunctionValue<'ctx>>,
    current_function_def_id: Option<DefId>,
    current_function_decls: Vec<LocalDecl>,
    /// 结构体类型映射
    struct_types: FxHashMap<DefId, inkwell::types::StructType<'ctx>>,

    is_main: bool,
    diagnostics: Vec<Diagnostic>,
}

impl<'ctx> CodeGen<'ctx> {
    pub fn new(context: &'ctx Context, mir_crate: MirCrate) -> Self {
        let module = context.create_module("main");
        let builder = context.create_builder();
        Self {
            context,
            module,
            builder,
            mir_crate,
            value_map: FxHashMap::default(),
            locals: FxHashMap::default(),
            current_function: None,
            current_function_def_id: None,
            current_function_decls: Vec::new(),
            struct_types: FxHashMap::default(),
            is_main: false,
            diagnostics: Vec::new(),
        }
    }

    pub fn generate(&mut self) {
        let items = std::mem::take(&mut self.mir_crate.items);

        // 声明所有结构体类型
        for item in &items {
            if let MirItem::Struct(mir_struct) = item {
                self.declare_struct(mir_struct);
            }
        }

        // 生成内置函数声明
        for builtin_function in std::mem::take(&mut self.mir_crate.builtin.functions) {
            self.generate_c_function(builtin_function);
        }

        // 生成所有项
        for item in items {
            self.generate_item(item);
        }
    }

    /// 声明结构体类型（不透明声明，后续填充）
    fn declare_struct(&mut self, mir_struct: &MirStruct) {
        let struct_name = self.get_def_id_name(mir_struct.def_id);
        let struct_ty = self.context.opaque_struct_type(&struct_name);
        self.struct_types.insert(mir_struct.def_id, struct_ty);
    }

    /// 编译为可执行文件（消耗 self）
    pub fn compile_to_binary(self, output_path: &Path) -> Result<(), String> {
        // 1. 生成临时目标文件
        let temp_dir = std::env::temp_dir();
        let obj_file = temp_dir.join(format!("temp_{}.o", std::process::id()));

        self.write_object_file(&obj_file)?;

        let linker = Linker::new().map_err(|e| format!("连接器初始失败: {}", e))?;

        linker
            .link_executable(&obj_file, output_path)
            .map_err(|e| format!("链接失败: {}", e))?;

        let _ = std::fs::remove_file(&obj_file);

        Ok(())
    }

    /// 仅生成目标文件
    fn write_object_file(&self, path: &Path) -> Result<(), String> {
        Target::initialize_all(&InitializationConfig::default());

        let triple = TargetMachine::get_default_triple();
        let target =
            Target::from_triple(&triple).map_err(|e| format!("Invalid target triple: {}", e))?;

        let machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                inkwell::OptimizationLevel::Default,
                RelocMode::Default,
                CodeModel::Default,
            )
            .ok_or("Failed to create target machine")?;

        machine
            .write_to_file(&self.module, FileType::Object, path)
            .map_err(|e| format!("Failed to write object file: {}", e))
    }

    /// 打印 LLVM IR（调试用）
    pub fn get_llvm_ir(&self) -> String {
        self.module.to_string()
    }

    fn generate_c_function(&mut self, builtin_function: BuiltinFunction) {
        let fn_name = get_global_string(builtin_function.name).unwrap();
        let ret_type = self.mir_type_to_llvm_type(&builtin_function.ret);

        let param_types: Vec<BasicMetadataTypeEnum<'ctx>> = builtin_function
            .params
            .iter()
            .map(|param| self.mir_type_to_llvm_type(&param.ty).into())
            .collect();

        let fn_type = ret_type.fn_type(&param_types, builtin_function.is_variadic);
        let function = self
            .module
            .add_function(&fn_name, fn_type, Some(Linkage::External));
        self.value_map.insert(builtin_function.def_id, function);
    }

    fn generate_item(&mut self, item: MirItem) {
        match item {
            MirItem::Function(mir_function) => self.generate_function(mir_function),
            MirItem::Struct(mir_struct) => self.generate_struct(mir_struct),
            MirItem::Use(mir_use) => self.generate_use(mir_use),
            MirItem::Module(mir_module) => self.generate_module(mir_module),
            MirItem::Extern(mir_extern) => self.generate_extern(mir_extern),
        }
    }

    fn generate_struct(&mut self, mir_struct: MirStruct) {
        // 获取或创建结构体类型
        let struct_ty = *self.struct_types.get(&mir_struct.def_id).unwrap();

        // 设置结构体字段
        let field_types: Vec<_> = mir_struct
            .fields
            .iter()
            .map(|field| self.mir_type_to_llvm_type(&field.ty))
            .collect();

        struct_ty.set_body(&field_types, false);
    }

    fn generate_use(&mut self, mir_use: MirUse) {
        let _ = mir_use;
    }

    fn generate_module(&mut self, mir_module: MirModule) {
        for item in mir_module.items {
            self.generate_item(item);
        }
    }

    fn generate_extern(&mut self, mir_extern: MirExtern) {
        for item in mir_extern.items {
            match item {
                MirExternItem::Function(func) => {
                    self.generate_extern_function(func, &mir_extern.abi);
                }
            }
        }
    }

    fn generate_extern_function(&mut self, func: MirExternFunction, abi: &AbiType) {
        if *abi == AbiType::Lite {
            return;
        }
        let name = get_global_string(func.name).unwrap();

        let linkage = Linkage::External;

        let ret_ty = match &func.return_ty {
            Some(ty) => self.mir_type_to_llvm_type(ty),
            None => self.context.struct_type(&[], false).into(),
        };

        let param_types: Vec<_> = func
            .params
            .iter()
            .map(|p| self.mir_type_to_llvm_type(&p.ty).into())
            .collect();

        let fn_type = if func.return_ty.is_some() {
            ret_ty.fn_type(&param_types, false)
        } else {
            self.context.void_type().fn_type(&param_types, false)
        };

        let function = self
            .module
            .add_function(name.as_ref(), fn_type, Some(linkage));

        self.value_map.insert(func.def_id, function);
    }

    fn generate_function(&mut self, mir_function: MirFunction) {
        let fn_name = self.get_def_id_name(mir_function.def_id);
        self.is_main = fn_name == "main";

        // 特殊处理 main 的签名
        let (llvm_ret_ty, llvm_params) = if self.is_main {
            let ret_ty = self.context.i32_type().into();
            let params: Vec<BasicMetadataTypeEnum<'ctx>> = if mir_function.args.is_empty() {
                vec![]
            } else {
                vec![
                    self.context.i32_type().into(),
                    self.context.ptr_type(AddressSpace::default()).into(),
                ]
            };
            (ret_ty, params)
        } else {
            let ret_ty = self.mir_type_to_llvm_type(&mir_function.return_ty);
            let params: Vec<_> = mir_function
                .args
                .iter()
                .map(|param| {
                    let local_decl = &mir_function.local_decls[param.0];
                    self.mir_type_to_llvm_type(&local_decl.ty).into()
                })
                .collect();
            (ret_ty, params)
        };

        let fn_type = llvm_ret_ty.fn_type(&llvm_params, false);
        let function = self
            .module
            .add_function(&fn_name, fn_type, Some(Linkage::External));

        self.value_map.insert(mir_function.def_id, function);
        self.current_function = Some(function);
        self.current_function_def_id = Some(mir_function.def_id);
        self.current_function_decls = mir_function.local_decls.clone();
        self.locals.clear();

        let entry_block = self.context.append_basic_block(function, "entry");
        self.builder.position_at_end(entry_block);

        // 分配局部变量并存储符号信息
        for (i, local_decl) in mir_function.local_decls.iter().enumerate() {
            let ty = self.mir_type_to_llvm_type(&local_decl.ty);
            let is_signed = self.is_signed_type(&local_decl.ty);
            let align = self.get_type_align(ty);

            let ptr = self
                .builder
                .build_alloca(
                    ty,
                    &format!(
                        "local_{}",
                        match local_decl.name {
                            Some(string_id) =>
                                get_global_string(string_id).unwrap().as_ref().to_string(),
                            None => i.to_string(),
                        }
                    ),
                )
                .unwrap();

            // 设置对齐
            if let Some(inst) = ptr.as_instruction() {
                match inst.set_alignment(align) {
                    Ok(_) => {}
                    Err(err) => {
                        self.diagnostics.push(
                            error(format!("对齐失败 {}", err.to_string()))
                                .with_span(mir_function.span)
                                .build(),
                        );
                        return;
                    }
                }
            }

            self.locals.insert(
                Local(i),
                PointerInfo {
                    ptr,
                    pointee_ty: ty,
                    is_signed,
                },
            );
        }

        // 存储参数
        for (i, param_local) in mir_function.args.iter().enumerate() {
            let param = function.get_nth_param(i as u32).unwrap();
            let info = self.locals.get(param_local).unwrap();
            let store = self.builder.build_store(info.ptr, param).unwrap();
            match store.set_alignment(self.get_type_align(info.pointee_ty)) {
                Ok(_) => {}
                Err(err) => {
                    self.diagnostics.push(
                        error(format!("对齐失败 {}", err.to_string()))
                            .with_span(mir_function.span)
                            .build(),
                    );
                    return;
                }
            }
        }

        // 创建基本块
        let mut block_map = FxHashMap::default();
        for i in 0..mir_function.basic_blocks.len() {
            let block = self
                .context
                .append_basic_block(function, &format!("basic_block_{}", i));
            block_map.insert(i, block);
        }

        if let Some(first) = block_map.get(&0) {
            self.builder.build_unconditional_branch(*first).unwrap();
        }

        for (i, mir_block) in mir_function.basic_blocks.iter().enumerate() {
            let block = block_map[&i];
            self.builder.position_at_end(block);
            self.generate_basic_block(mir_block, &block_map);
        }
    }

    fn get_type_align(&self, ty: BasicTypeEnum<'ctx>) -> u32 {
        match ty {
            t if t.is_int_type() => {
                let bits = t.into_int_type().get_bit_width();
                ((bits / 8).next_power_of_two()).max(1) as u32
            }
            t if t.is_float_type() => {
                if t.into_float_type() == self.context.f32_type() {
                    4
                } else {
                    8
                }
            }
            t if t.is_pointer_type() => 8,
            t if t.is_array_type() => {
                let elem_ty = t.into_array_type().get_element_type();
                self.get_type_align(elem_ty)
            }
            t if t.is_struct_type() => {
                let struct_ty = t.into_struct_type();
                (0..struct_ty.get_field_types().len())
                    .map(|i| {
                        self.get_type_align(struct_ty.get_field_type_at_index(i as u32).unwrap())
                    })
                    .max()
                    .unwrap_or(1)
            }
            _ => 8,
        }
    }

    fn generate_basic_block(
        &mut self,
        mir_block: &BasicBlock,
        block_map: &FxHashMap<usize, inkwell::basic_block::BasicBlock<'ctx>>,
    ) {
        for statement in &mir_block.statements {
            self.generate_statement(statement);
        }
        self.generate_terminator(&mir_block.terminator, block_map);
    }

    fn generate_statement(&mut self, statement: &Statement) {
        match &statement.kind {
            StatementKind::Assign { place, rvalue } => {
                let addr = self.emit_place_address(place);
                let value = self.emit_rvalue(rvalue, Some(&place.ty));
                self.builder.build_store(addr, value).unwrap();
            }
            StatementKind::Nop => {}
        }
    }

    fn generate_terminator(
        &mut self,
        terminator: &Terminator,
        block_map: &FxHashMap<usize, inkwell::basic_block::BasicBlock<'ctx>>,
    ) {
        match &terminator.kind {
            TerminatorKind::Return { value, is_explicit } => {
                if self.is_main && !is_explicit {
                    let value = self.context.i32_type().const_int(0, false);
                    self.builder.build_return(Some(&value)).unwrap();
                    return;
                }
                let operand_val = self.emit_operand(value);
                self.builder.build_return(Some(&operand_val)).unwrap();
            }
            TerminatorKind::Goto { target } => {
                let target_block = block_map[&target.0];
                self.builder
                    .build_unconditional_branch(target_block)
                    .unwrap();
            }
            TerminatorKind::SwitchInt { discr, targets } => {
                let discr_val = self.emit_operand(discr).into_int_value();
                let otherwise_block = block_map[&targets.otherwise.0];

                let cases: Vec<_> = targets
                    .values
                    .iter()
                    .zip(targets.targets.iter())
                    .map(|(val, target)| {
                        let llvm_val = self.context.i64_type().const_int(*val as u64, false);
                        (llvm_val, block_map[&target.0])
                    })
                    .collect();

                self.builder
                    .build_switch(discr_val, otherwise_block, &cases)
                    .unwrap();
            }
            TerminatorKind::Call {
                function,
                args,
                destination,
                target,
            } => {
                let function_val = match self.value_map.get(&function).copied() {
                    Some(function) => function,
                    None => {
                        self.diagnostics.push(
                            error(format!("未知函数 {}", self.get_def_id_name(*function)))
                                .with_span(terminator.span)
                                .build(),
                        );
                        return;
                    }
                };

                let mut arg_vals = Vec::with_capacity(args.len());
                for arg in args {
                    let val = self.emit_operand(arg);
                    arg_vals.push(val.into());
                }

                let call_site = self
                    .builder
                    .build_call(function_val, &arg_vals, "call")
                    .unwrap();

                let dest_addr = self.emit_place_address(destination);
                match call_site.try_as_basic_value() {
                    ValueKind::Basic(basic_value) => {
                        self.builder.build_store(dest_addr, basic_value).unwrap();
                    }
                    ValueKind::Instruction(_) => {}
                }

                let next_block = block_map[&target.0];
                self.builder.build_unconditional_branch(next_block).unwrap();
            }
        }
    }

    /// 统一处理 Place
    fn emit_place_address(&mut self, place: &Place) -> PointerValue<'ctx> {
        let base = *self.locals.get(&place.local).unwrap();

        // 检查是否以 Deref 开头
        if let Some(PlaceElem::Deref) = place.projection.first() {
            // 加载指针值，得到目标地址
            let ptr_val = self
                .builder
                .build_load(base.pointee_ty, base.ptr, "ptr_val")
                .unwrap()
                .into_pointer_value();

            // 处理剩余投影
            self.apply_projection(ptr_val, &place.projection[1..], place.local)
        } else {
            // 普通变量
            self.apply_projection(base.ptr, &place.projection, place.local)
        }
    }

    /// 应用投影（Field, Index 等）
    fn apply_projection(
        &mut self,
        base_ptr: PointerValue<'ctx>,
        projection: &[PlaceElem],
        base_local: Local,
    ) -> PointerValue<'ctx> {
        let mut current_ptr = base_ptr;
        let mut current_mir_ty = self.get_local_ty(base_local);

        for elem in projection {
            match elem {
                PlaceElem::Field(field) => {
                    let struct_ty = self
                        .mir_type_to_llvm_type(&current_mir_ty)
                        .into_struct_type();

                    current_ptr = self
                        .builder
                        .build_struct_gep(
                            struct_ty,
                            current_ptr,
                            field.index as u32,
                            &format!("field_{}", field.index),
                        )
                        .unwrap();
                    // 更新 current_mir_ty 为字段类型
                    current_mir_ty = field.ty.clone();
                }
                PlaceElem::Index(local) => {
                    let index_info = self.locals.get(local).unwrap();
                    let index_val = self
                        .builder
                        .build_load(index_info.pointee_ty, index_info.ptr, "index")
                        .unwrap()
                        .into_int_value();

                    let array_ty = self.mir_type_to_llvm_type(&current_mir_ty);

                    current_ptr = unsafe {
                        self.builder
                            .build_in_bounds_gep(
                                array_ty,
                                current_ptr,
                                &[self.context.i32_type().const_int(0, false), index_val],
                                "elem",
                            )
                            .unwrap()
                    };
                    current_mir_ty = self.get_element_ty(&current_mir_ty);
                }
                PlaceElem::ConstantIndex { offset, .. } => {
                    let index_val = self.context.i64_type().const_int(*offset as u64, false);
                    let array_ty = self.mir_type_to_llvm_type(&current_mir_ty);

                    current_ptr = unsafe {
                        self.builder
                            .build_in_bounds_gep(
                                array_ty,
                                current_ptr,
                                &[self.context.i32_type().const_int(0, false), index_val],
                                &format!("const_index_{}", offset),
                            )
                            .unwrap()
                    };
                    current_mir_ty = self.get_element_ty(&current_mir_ty);
                }
                PlaceElem::Deref => {
                    let loaded = self
                        .builder
                        .build_load(
                            self.mir_type_to_llvm_type(&current_mir_ty),
                            current_ptr,
                            "deref",
                        )
                        .unwrap();
                    current_ptr = loaded.into_pointer_value();
                    current_mir_ty = self.get_pointee_ty(&current_mir_ty);
                }
            }
        }

        current_ptr
    }

    fn get_local_ty(&self, local: Local) -> Ty {
        self.current_function_decls
            .get(local.0)
            .map(|decl| decl.ty.clone())
            .unwrap_or_else(|| {
                panic!(
                    "Local {} not found, len={}",
                    local.0,
                    self.current_function_decls.len()
                )
            })
    }

    fn get_element_ty(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Array { element, .. } => element.clone().as_ref().clone(),
            Ty::Ptr(to) | Ty::Ref { to, .. } => *to.clone(),
            _ => panic!("not an array or pointer type"),
        }
    }

    fn emit_rvalue(&mut self, rvalue: &Rvalue, target_ty: Option<&Ty>) -> BasicValueEnum<'ctx> {
        match rvalue {
            Rvalue::Use(operand) => self.emit_operand(operand),
            Rvalue::BinaryOp(op, left, right) => {
                let left_val = self.emit_operand(left);
                let right_val = self.emit_operand(right);
                let left_info = self.get_operand_type_info(left);
                let right_info = self.get_operand_type_info(right);
                self.emit_binary_op(*op, left_val, right_val, left_info, right_info)
            }
            Rvalue::UnaryOp(op, operand) => {
                let val = self.emit_operand(operand);
                self.emit_unary_op(*op, val)
            }
            Rvalue::Ref(_, place) => {
                let addr = self.emit_place_address(place);
                addr.into()
            }
            Rvalue::Deref(place) => {
                // Deref 作为右值：加载值
                let addr = self.emit_place_address(place);
                let ty = self.mir_type_to_llvm_type(&place.ty);
                self.builder.build_load(ty, addr, "deref_load").unwrap()
            }
            Rvalue::AddressOf(place) => {
                let addr = self.emit_place_address(place);
                addr.into()
            }
            Rvalue::Aggregate(kind, operands) => self.emit_aggregate(kind, operands).unwrap(),
            Rvalue::Cast(cast_kind, place) => {
                let source_addr = self.emit_place_address(place);
                let source_ty = self.mir_type_to_llvm_type(&place.ty);
                let val = self
                    .builder
                    .build_load(source_ty, source_addr, "cast_src")
                    .unwrap();

                let to_ty =
                    self.mir_type_to_llvm_type(target_ty.expect("Cast requires target type"));
                let from_info = self.get_place_type_info(place);
                let to_info = TypeInfo {
                    llvm_type: to_ty,
                    ty: target_ty.unwrap().clone(),
                };

                self.emit_cast(*cast_kind, val, from_info, to_info)
            }
        }
    }

    fn emit_cast(
        &mut self,
        kind: CastKind,
        val: BasicValueEnum<'ctx>,
        from: TypeInfo<'ctx>,
        to: TypeInfo<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        match kind {
            CastKind::Identity => val,
            CastKind::SignExtend => self
                .builder
                .build_int_s_extend(val.into_int_value(), to.llvm_type.into_int_type(), "sext")
                .unwrap()
                .into(),
            CastKind::ZeroExtend => self
                .builder
                .build_int_z_extend(val.into_int_value(), to.llvm_type.into_int_type(), "zext")
                .unwrap()
                .into(),
            CastKind::Truncate => self
                .builder
                .build_int_truncate(
                    val.into_int_value(),
                    to.llvm_type.into_int_type(),
                    "truncate",
                )
                .unwrap()
                .into(),
            CastKind::IntToFloat => {
                if matches!(from.ty, Ty::Int(_)) {
                    self.builder
                        .build_signed_int_to_float(
                            val.into_int_value(),
                            to.llvm_type.into_float_type(),
                            "int_to_float",
                        )
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_unsigned_int_to_float(
                            val.into_int_value(),
                            to.llvm_type.into_float_type(),
                            "uint_to_float",
                        )
                        .unwrap()
                        .into()
                }
            }
            CastKind::UintToFloat => self
                .builder
                .build_unsigned_int_to_float(
                    val.into_int_value(),
                    to.llvm_type.into_float_type(),
                    "unit_to_float",
                )
                .unwrap()
                .into(),
            CastKind::FloatToInt => {
                if matches!(to.ty, Ty::Int(_)) {
                    self.builder
                        .build_float_to_signed_int(
                            val.into_float_value(),
                            to.llvm_type.into_int_type(),
                            "float_to_int",
                        )
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_to_unsigned_int(
                            val.into_float_value(),
                            to.llvm_type.into_int_type(),
                            "float_to_uint",
                        )
                        .unwrap()
                        .into()
                }
            }
            CastKind::FloatToUint => self
                .builder
                .build_float_to_unsigned_int(
                    val.into_float_value(),
                    to.llvm_type.into_int_type(),
                    "float_to_unsigned_int",
                )
                .unwrap()
                .into(),
            CastKind::FloatPromote => self
                .builder
                .build_float_ext(
                    val.into_float_value(),
                    to.llvm_type.into_float_type(),
                    "float_ext",
                )
                .unwrap()
                .into(),
            CastKind::FloatDemote => self
                .builder
                .build_float_trunc(
                    val.into_float_value(),
                    to.llvm_type.into_float_type(),
                    "float_trunc",
                )
                .unwrap()
                .into(),
            CastKind::PtrToPtr => self
                .builder
                .build_pointer_cast(
                    val.into_pointer_value(),
                    to.llvm_type.into_pointer_type(),
                    "ptr_to_ptr",
                )
                .unwrap()
                .into(),
            CastKind::PtrToInt => self
                .builder
                .build_ptr_to_int(
                    val.into_pointer_value(),
                    to.llvm_type.into_int_type(),
                    "ptr_to_int",
                )
                .unwrap()
                .into(),
            CastKind::IntToPtr => self
                .builder
                .build_int_to_ptr(
                    val.into_int_value(),
                    to.llvm_type.into_pointer_type(),
                    "int_to_ptr",
                )
                .unwrap()
                .into(),
            CastKind::Bitcast => self
                .builder
                .build_bit_cast(val, to.llvm_type, "bit_cast")
                .unwrap()
                .into(),
        }
    }

    fn emit_aggregate(
        &mut self,
        kind: &AggregateKind,
        operands: &[Operand],
    ) -> Option<BasicValueEnum<'ctx>> {
        let llvm_ty = match kind {
            AggregateKind::Array(elem_ty) => {
                let elem_type = self.mir_type_to_llvm_type(elem_ty);
                elem_type
                    .array_type(operands.len() as u32)
                    .as_basic_type_enum()
            }
            AggregateKind::Tuple => {
                let field_types: Vec<_> = operands
                    .iter()
                    .map(|op| self.get_operand_type(op))
                    .collect();
                self.context
                    .struct_type(&field_types, false)
                    .as_basic_type_enum()
            }
            AggregateKind::Adt(def_id, variant_idx) => {
                let struct_name = self.get_def_id_name(*def_id);
                let name = if !variant_idx.is_empty() && variant_idx[0] > 0 {
                    format!("{}_{}", struct_name, variant_idx[0])
                } else {
                    struct_name
                };
                self.module
                    .get_struct_type(&name)
                    .unwrap_or_else(|| self.context.opaque_struct_type(&name))
                    .as_basic_type_enum()
            }
        };

        let temp_ptr = self
            .builder
            .build_alloca(llvm_ty, "aggregate_temp")
            .unwrap();

        for (i, operand) in operands.iter().enumerate() {
            let field_val = self.emit_operand(operand);
            let field_ptr = match kind {
                AggregateKind::Adt(_, _) | AggregateKind::Tuple => self
                    .builder
                    .build_struct_gep(
                        llvm_ty.into_struct_type(),
                        temp_ptr,
                        i as u32,
                        &format!("field_{}", i),
                    )
                    .unwrap(),
                AggregateKind::Array(_) => unsafe {
                    self.builder
                        .build_in_bounds_gep(
                            llvm_ty,
                            temp_ptr,
                            &[
                                self.context.i32_type().const_int(0, false),
                                self.context.i32_type().const_int(i as u64, false),
                            ],
                            &format!("elem_{}", i),
                        )
                        .unwrap()
                },
            };
            self.builder.build_store(field_ptr, field_val).unwrap();
        }

        Some(
            self.builder
                .build_load(llvm_ty, temp_ptr, "agg_val")
                .unwrap(),
        )
    }

    fn get_operand_type(&self, operand: &Operand) -> BasicTypeEnum<'ctx> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                self.mir_type_to_llvm_type(&self.get_local_ty(place.local))
            }
            Operand::Constant(constant) => {
                let ty = match &constant.kind {
                    ConstantKind::Literal { ty, .. } => ty,
                    ConstantKind::Global { ty, .. } => ty,
                    ConstantKind::Function { ty, .. } => ty,
                    ConstantKind::Unit => &Ty::Unit,
                };
                self.mir_type_to_llvm_type(ty)
            }
        }
    }

    fn get_pointee_ty(&self, ty: &Ty) -> Ty {
        match ty {
            Ty::Ptr(inner) => *inner.clone(),
            Ty::Ref { to, .. } => *to.clone(),
            _ => panic!("expected pointer or reference type, got {:?}", ty),
        }
    }

    fn emit_operand(&mut self, operand: &Operand) -> BasicValueEnum<'ctx> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                let addr = self.emit_place_address(place);
                let ty = self.mir_type_to_llvm_type(&place.ty);
                self.builder.build_load(ty, addr, "load").unwrap()
            }
            Operand::Constant(constant) => self.emit_constant(constant),
        }
    }

    fn emit_binary_op(
        &mut self,
        op: BinOp,
        left: BasicValueEnum<'ctx>,
        right: BasicValueEnum<'ctx>,
        left_info: TypeInfo<'ctx>,
        right_info: TypeInfo<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        // 处理整数操作
        if left_info.llvm_type.is_int_type() && right_info.llvm_type.is_int_type() {
            self.emit_int_binary_op(op, left.into_int_value(), right.into_int_value(), left_info)
        } else if left_info.llvm_type.is_float_type() && right_info.llvm_type.is_float_type() {
            self.emit_float_binary_op(op, left.into_float_value(), right.into_float_value())
        } else {
            panic!("Unsupported operand types for binary operation");
        }
    }

    /// 整数二元操作（处理有符号/无符号）
    fn emit_int_binary_op(
        &self,
        op: BinOp,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        left_info: TypeInfo<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        match op {
            BinOp::Div => {
                if matches!(left_info.ty, Ty::Int(_)) {
                    self.builder.build_int_signed_div(left, right, "sdiv")
                } else {
                    self.builder.build_int_unsigned_div(left, right, "udiv")
                }
            }
            BinOp::Rem => {
                if matches!(left_info.ty, Ty::Int(_)) {
                    self.builder.build_int_signed_rem(left, right, "srem")
                } else {
                    self.builder.build_int_unsigned_rem(left, right, "urem")
                }
            }
            BinOp::Shr => {
                if matches!(left_info.ty, Ty::Int(_)) {
                    self.builder.build_right_shift(left, right, true, "ashr")
                } else {
                    self.builder.build_right_shift(left, right, false, "lshr")
                }
            }
            BinOp::Lt => {
                let pred = if matches!(left_info.ty, Ty::Int(_)) {
                    IntPredicate::SLT
                } else {
                    IntPredicate::ULT
                };
                self.builder.build_int_compare(pred, left, right, "lt")
            }
            BinOp::Le => {
                let pred = if matches!(left_info.ty, Ty::Int(_)) {
                    IntPredicate::SLE
                } else {
                    IntPredicate::ULE
                };
                self.builder.build_int_compare(pred, left, right, "le")
            }
            BinOp::Gt => {
                let pred = if matches!(left_info.ty, Ty::Int(_)) {
                    IntPredicate::SGT
                } else {
                    IntPredicate::UGT
                };
                self.builder.build_int_compare(pred, left, right, "gt")
            }
            BinOp::Ge => {
                let pred = if matches!(left_info.ty, Ty::Int(_)) {
                    IntPredicate::SGE
                } else {
                    IntPredicate::UGE
                };
                self.builder.build_int_compare(pred, left, right, "ge")
            }
            // 符号无关的操作
            BinOp::Add => self.builder.build_int_add(left, right, "add"),
            BinOp::Sub => self.builder.build_int_sub(left, right, "sub"),
            BinOp::Mul => self.builder.build_int_mul(left, right, "mul"),
            BinOp::BitAnd => self.builder.build_and(left, right, "and"),
            BinOp::BitOr => self.builder.build_or(left, right, "or"),
            BinOp::BitXor => self.builder.build_xor(left, right, "xor"),
            BinOp::Shl => self.builder.build_left_shift(left, right, "shl"),
            BinOp::Eq => self
                .builder
                .build_int_compare(IntPredicate::EQ, left, right, "eq"),
            BinOp::Ne => self
                .builder
                .build_int_compare(IntPredicate::NE, left, right, "ne"),
            _ => panic!("Unsupported binary op for integers: {:?}", op),
        }
        .unwrap()
        .into()
    }

    /// 浮点数二元操作
    fn emit_float_binary_op(
        &self,
        op: BinOp,
        left: FloatValue<'ctx>,
        right: FloatValue<'ctx>,
    ) -> BasicValueEnum<'ctx> {
        let result = match op {
            BinOp::Add => self
                .builder
                .build_float_add(left, right, "fadd")
                .unwrap()
                .as_basic_value_enum(),
            BinOp::Sub => self
                .builder
                .build_float_sub(left, right, "fsub")
                .unwrap()
                .as_basic_value_enum(),
            BinOp::Mul => self
                .builder
                .build_float_mul(left, right, "fmul")
                .unwrap()
                .as_basic_value_enum(),
            BinOp::Div => self
                .builder
                .build_float_div(left, right, "fdiv")
                .unwrap()
                .as_basic_value_enum(),
            BinOp::Rem => self
                .builder
                .build_float_rem(left, right, "frem")
                .unwrap()
                .as_basic_value_enum(),
            BinOp::Lt => self
                .builder
                .build_float_compare(inkwell::FloatPredicate::ULT, left, right, "flt")
                .unwrap()
                .as_basic_value_enum(),
            BinOp::Le => self
                .builder
                .build_float_compare(inkwell::FloatPredicate::ULE, left, right, "fle")
                .unwrap()
                .as_basic_value_enum(),
            BinOp::Gt => self
                .builder
                .build_float_compare(inkwell::FloatPredicate::UGT, left, right, "fgt")
                .unwrap()
                .as_basic_value_enum(),
            BinOp::Ge => self
                .builder
                .build_float_compare(inkwell::FloatPredicate::UGE, left, right, "fge")
                .unwrap()
                .as_basic_value_enum(),
            BinOp::Eq => self
                .builder
                .build_float_compare(inkwell::FloatPredicate::UEQ, left, right, "feq")
                .unwrap()
                .as_basic_value_enum(),
            BinOp::Ne => self
                .builder
                .build_float_compare(inkwell::FloatPredicate::UNE, left, right, "fne")
                .unwrap()
                .as_basic_value_enum(),
            _ => panic!("Unsupported binary op for floats: {:?}", op),
        };

        result
    }

    fn emit_unary_op(&mut self, op: UnOp, operand: BasicValueEnum<'ctx>) -> BasicValueEnum<'ctx> {
        match op {
            UnOp::Neg => {
                if operand.is_int_value() {
                    self.builder
                        .build_int_neg(operand.into_int_value(), "neg")
                        .unwrap()
                        .into()
                } else {
                    self.builder
                        .build_float_neg(operand.into_float_value(), "fneg")
                        .unwrap()
                        .into()
                }
            }
            UnOp::Not => {
                let v = operand.into_int_value();
                self.builder.build_not(v, "not").unwrap().into()
            }
        }
    }

    fn emit_constant(&mut self, constant: &Constant) -> BasicValueEnum<'ctx> {
        match &constant.kind {
            ConstantKind::Literal { value, ty } => {
                let info = self.get_type_info(ty);
                self.emit_literal(value, info)
            }
            ConstantKind::Global { def_id, ty } => self.emit_global(*def_id, ty),
            ConstantKind::Function { def_id, ty: _ } => {
                self.emit_function(*def_id, constant.span).unwrap()
            }
            ConstantKind::Unit => self.context.i32_type().const_int(0, false).into(),
        }
    }

    fn emit_literal(&mut self, value: &LiteralValue, info: TypeInfo<'ctx>) -> BasicValueEnum<'ctx> {
        match value {
            LiteralValue::Int { value, kind } => {
                let is_signed = matches!(kind, LiteralIntKind::Signed(_));
                info.llvm_type
                    .into_int_type()
                    .const_int(*value, is_signed)
                    .into()
            }
            LiteralValue::Bool(b) => self.context.bool_type().const_int(*b as u64, false).into(),
            LiteralValue::Char(c) => self.context.i8_type().const_int(*c as u64, false).into(),
            LiteralValue::Str(s) => {
                let global = unsafe {
                    self.builder
                        .build_global_string(&get_global_string(*s).unwrap(), "str")
                        .unwrap()
                };
                global.as_pointer_value().into()
            }
            LiteralValue::Float { value, kind } => {
                let float_ty = info.llvm_type.into_float_type();
                float_ty.const_float(*value).into()
            }
            LiteralValue::Unit => self.context.i32_type().const_int(0, false).into(),
        }
    }

    fn emit_global(&mut self, def_id: DefId, ty: &Ty) -> BasicValueEnum<'ctx> {
        let name = self.get_def_id_name(def_id);
        let global = self.module.get_global(&name).expect("Global not found");
        let ptr = global.as_pointer_value();
        let pointee_ty = self.mir_type_to_llvm_type(ty);
        self.builder.build_load(pointee_ty, ptr, "global").unwrap()
    }

    fn emit_function(&mut self, def_id: DefId, span: Span) -> Option<BasicValueEnum<'ctx>> {
        let fn_val = match self.value_map.get(&def_id) {
            Some(fn_val) => fn_val,
            _ => {
                self.diagnostics.push(
                    error(format!("无法找到函数 `{}`", self.get_def_id_name(def_id)))
                        .with_span(span)
                        .build(),
                );
                return None;
            }
        };
        Some(fn_val.as_global_value().as_pointer_value().into())
    }

    fn mir_type_to_llvm_type(&self, ty: &Ty) -> BasicTypeEnum<'ctx> {
        match ty {
            Ty::Int(int_kind) => self
                .context
                .custom_width_int_type(int_kind.bit_width())
                .into(),
            Ty::UInt(int_kind) => self
                .context
                .custom_width_int_type(int_kind.bit_width())
                .into(),
            Ty::Float(FloatKind::F32) => self.context.f32_type().into(),
            Ty::Float(FloatKind::F64) => self.context.f64_type().into(),
            Ty::Bool => self.context.bool_type().into(),
            Ty::Char => self.context.i8_type().into(),
            Ty::Str
            | Ty::RawPtr
            | Ty::Ptr(_)
            | Ty::Ref { .. }
            | Ty::Fn { .. }
            | Ty::ExternFn { .. } => self.context.ptr_type(AddressSpace::default()).into(),
            Ty::Array { element, len } => {
                let elem_ty = self.mir_type_to_llvm_type(element);
                match len {
                    Some(n) => elem_ty.array_type(*n as u32).into(),
                    None => self.context.ptr_type(AddressSpace::default()).into(),
                }
            }
            Ty::Tuple(elems) => {
                if elems.is_empty() {
                    self.context.struct_type(&[], false).into()
                } else {
                    let field_tys: Vec<_> = elems
                        .iter()
                        .map(|t| self.mir_type_to_llvm_type(t))
                        .collect();
                    self.context.struct_type(&field_tys, false).into()
                }
            }
            Ty::Adt(def_id) => {
                let name = self.get_def_id_name(*def_id);
                self.module
                    .get_struct_type(&name)
                    .unwrap_or_else(|| self.context.opaque_struct_type(&name))
                    .into()
            }
            Ty::Range { ty } => {
                let inner = self.mir_type_to_llvm_type(ty);
                self.context.struct_type(&[inner, inner], false).into()
            }
            Ty::Unit | Ty::Never | Ty::Error => self.context.struct_type(&[], false).into(),
            Ty::SelfType => self.context.i32_type().into(),
            Ty::Unknown => self.context.i32_type().into(),
        }
    }

    /// 获取操作数的类型信息
    fn get_operand_type_info(&self, operand: &Operand) -> TypeInfo<'ctx> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.get_place_type_info(place),
            Operand::Constant(constant) => {
                let ty = match &constant.kind {
                    ConstantKind::Literal { ty, .. } => ty,
                    ConstantKind::Global { ty, .. } => ty,
                    ConstantKind::Function { ty, .. } => ty,
                    ConstantKind::Unit => &Ty::Unit,
                };
                self.get_type_info(ty)
            }
        }
    }

    /// 获取位置的类型信息
    fn get_place_type_info(&self, place: &Place) -> TypeInfo<'ctx> {
        let ty = &place.ty;
        self.get_type_info(ty)
    }

    /// 获取类型的完整信息
    fn get_type_info(&self, ty: &Ty) -> TypeInfo<'ctx> {
        let llvm_type = self.mir_type_to_llvm_type(ty);
        TypeInfo {
            llvm_type,
            ty: ty.clone(),
        }
    }

    /// 判断是否为有符号类型
    fn is_signed_type(&self, ty: &Ty) -> bool {
        matches!(ty, Ty::Int(_))
    }

    fn get_def_id_name(&self, def_id: DefId) -> String {
        get_global_string(self.mir_crate.definitions[def_id.index as usize].name)
            .unwrap()
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Context;
    use litec_lower::lower;
    use litec_mir_lower::build;
    use litec_name_resolver::resolve;
    use litec_parse::parser::parse;
    use litec_span::SourceMap;
    use litec_type_checker::check;

    use crate::CodeGen;

    #[test]
    fn test() {
        let source = r#"
        extern "Lite" {
            fn printf(fmt: str, ...) -> i32;
        }

        fn add(a: i32, b: i32) -> i32 {
            return a + b;
        }


        fn main() {
            let a = add(1, 2);
            printf("%d\n", a);
        }"#;
        let mut source_map = SourceMap::new();
        let file_id = source_map.add_file("test.lt".into(), source.to_string(), &Path::new(""));
        let (ast, diagnostics) = parse(&source_map, file_id);
        for diagnostic in diagnostics {
            println!("{}", diagnostic.render(&source_map));
        }
        let (hir, diagnostics) = lower(ast);
        for diagnostic in diagnostics {
            println!("{}", diagnostic.render(&source_map));
        }
        let resolve_output = resolve(hir, &mut source_map, file_id);
        for diagnostic in &resolve_output.diagnostics {
            println!("{}", diagnostic.render(&source_map));
        }
        let (typed_hir, diagnostics) = check(resolve_output);
        for diagnostic in diagnostics {
            println!("{}", diagnostic.render(&source_map));
        }
        let mir_crate = build(typed_hir);
        let context = Context::create();
        let mut codegen = CodeGen::new(&context, mir_crate);
        codegen.generate();
    }
}
