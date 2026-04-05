// src/main.rs

mod cli;

use std::process::Command;
use std::{panic, path::PathBuf};

use anyhow::{Context, Ok, Result, anyhow};
use cli::{Cli, Commands, OptLevel};

use litec_codegen::CodeGen;
use litec_lower::lower;
use litec_mir::MirCrate;
use litec_mir_lower::build;
use litec_name_resolver::resolve;
use litec_parse::parser::parse;
use litec_span::SourceMap;
use litec_type_checker::check;

fn main() -> Result<()> {
    
    let cli = Cli::parse_args();

    match cli.command {
        Commands::Build {
            input,
            output,
            opt_level,
            compile_only,
            out_dir,
            emit_llvm,
            emit_mir,
            library_path: _,
            library: _,
        } => build_command(
            input,
            output,
            opt_level,
            cli.verbose,
            compile_only,
            out_dir,
            emit_llvm,
            emit_mir,
        ),
        Commands::Parse {
            input,
            ast,
            hir,
            tokens,
            output,
        } => parse_command(input, ast, hir, tokens, output),
        Commands::Mir {
            input,
            output,
            typed_hir,
            optimized,
        } => mir_command(input, output, typed_hir, optimized),
        Commands::Run {
            input,
            args,
            opt_level,
        } => run_command(input, args, opt_level),
        Commands::Check {
            input,
            deny_warnings,
            syntax_only,
        } => check_command(input, deny_warnings, syntax_only),
        Commands::Clean { dir, profile } => clean_command(dir, profile),
        Commands::Test {
            filter,
            nocapture,
            jobs,
        } => test_command(filter, nocapture, jobs),
    }
}

/// 构建命令：编译源文件为可执行文件或目标文件
fn build_command(
    input: PathBuf,
    output: Option<PathBuf>,
    opt_level: OptLevel,
    verbose: bool,
    compile_only: bool,
    out_dir: Option<PathBuf>,
    emit_llvm: bool,
    emit_mir: bool,
) -> Result<()> {
    if verbose {
        println!("🔨 构建模式");
        println!("📄 输入文件: {}", input.display());
        println!("⚡ 优化级别: {:?}", opt_level);
    }

    // 读取源代码
    let source_code = std::fs::read_to_string(&input)
        .with_context(|| format!("无法读取文件: {}", input.display()))?;

    // 确定输出路径
    let output_path = if let Some(out) = output {
        out
    } else if compile_only {
        input.with_extension(if cfg!(windows) { "obj" } else { "o" })
    } else {
        PathBuf::from("a.out")
    };

    // 编译流程
    let (mir, source_map) = compile_to_mir(&source_code, &input, verbose)?;

    // 输出 MIR（如果需要）
    if emit_mir {
        let mir_path = input.with_extension("mir");
        let mir_text = format!("{:#?}", mir);
        std::fs::write(&mir_path, mir_text)?;
        println!("💾 MIR 已保存到: {}", mir_path.display());
        return Ok(());
    }

    // 代码生成
    if verbose {
        println!("🔧 开始代码生成...");
    }

    let context = inkwell::context::Context::create();
    let mut codegen = CodeGen::new(&context, mir);
    codegen.generate();

    // 输出 LLVM IR（如果需要）
    if emit_llvm {
        let llvm_path = input.with_extension("ll");
        let llvm_ir = codegen.get_llvm_ir();
        std::fs::write(&llvm_path, llvm_ir)?;
        println!("💾 LLVM IR 已保存到: {}", llvm_path.display());
        return Ok(());
    }

    if compile_only {
        // 只生成目标文件
        let obj_path = output_path;
        codegen
            .compile_to_binary(&obj_path)
            .map_err(|e| anyhow!("代码生成失败: {}", e))?;
        println!("✅ 目标文件: {}", obj_path.display());
    } else {
        // 生成可执行文件
        codegen
            .compile_to_binary(&output_path)
            .map_err(|e| anyhow!("编译失败: {}", e))?;
        println!("✅ 可执行文件: {}", output_path.display());
    }

    Ok(())
}

/// 解析命令：语法分析和 AST 显示
fn parse_command(
    input: PathBuf,
    show_ast: bool,
    show_hir: bool,
    show_tokens: bool,
    output: Option<PathBuf>,
) -> Result<()> {
    println!("🔍 解析: {}", input.display());

    let source_code = std::fs::read_to_string(&input)
        .with_context(|| format!("无法读取文件: {}", input.display()))?;

    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file(
        input.as_os_str().to_str().unwrap().to_string(),
        source_code.clone(),
        &input.clone(),
    );

    // 词法分析
    if show_tokens {
        println!("\n=== Tokens ===");
        // TODO: 实现 token 显示
    }

    // 语法分析
    let (ast, diagnostics) = parse(&source_map, file_id);

    if !diagnostics.is_empty() {
        eprintln!("❌ 解析错误:");
        for diag in &diagnostics {
            eprintln!("  {}", diag.render(&source_map));
        }
        return Err(anyhow!("解析失败"));
    }

    let ast_str = format!("{:#?}", ast);

    // HIR 转换
    let (hir, lower_diagnostics) = lower(ast);

    // 输出结果
    let output_text = if show_ast && show_hir {
        format!("=== AST ===\n{:#?}\n\n=== HIR ===\n{:#?}", ast_str, hir)
    } else if show_ast {
        format!("{:#?}", ast_str)
    } else if show_hir {
        format!("{:#?}", hir)
    } else {
        "解析成功！使用 --ast 或 --hir 查看详细信息".to_string()
    };

    if !lower_diagnostics.is_empty() {
        eprintln!("⚠️  HIR 错误:");
        for diag in &lower_diagnostics {
            eprintln!("  {}", diag.render(&source_map));
        }
    }

    match output {
        Some(path) => {
            std::fs::write(&path, output_text)?;
            println!("💾 输出已保存到: {}", path.display());
        }
        None => {
            println!("{}", output_text);
        }
    }

    Ok(())
}

/// MIR 命令：生成并显示 MIR
fn mir_command(
    input: PathBuf,
    output: Option<PathBuf>,
    show_typed_hir: bool,
    optimized: bool,
) -> Result<()> {
    println!("🔄 MIR 生成: {}", input.display());

    let source_code = std::fs::read_to_string(&input)
        .with_context(|| format!("无法读取文件: {}", input.display()))?;

    let (mir, _source_map) = compile_to_mir(&source_code, &input, false)?;

    if show_typed_hir {
        println!("⚠️  显示 Typed HIR 需要修改编译流程");
    }

    if optimized {
        println!("⚠️  MIR 优化暂未实现");
    }

    let mir_text = format!("{:#?}", mir);

    match output {
        Some(path) => {
            std::fs::write(&path, mir_text)?;
            println!("💾 MIR 已保存到: {}", path.display());
        }
        None => {
            println!("\n=== MIR ===\n{}", mir_text);
        }
    }

    Ok(())
}

/// 运行命令：编译并立即执行
fn run_command(input: PathBuf, args: Vec<String>, opt_level: OptLevel) -> Result<()> {
    println!("🏃 运行: {}", input.display());

    // 编译到临时可执行文件
    let temp_exe = std::env::temp_dir().join(format!(
        "litec_run_{}_{}",
        input.file_stem().unwrap().to_string_lossy(),
        std::process::id()
    ));

    build_command(
        input,
        Some(temp_exe.clone()),
        opt_level,
        false, // verbose
        false, // compile_only
        None,  // out_dir
        false, // emit_llvm
        false, // emit_mir
    )?;

    // 执行程序
    println!("▶️  执行: {} {:?}", temp_exe.display(), args);

    let status = Command::new(&temp_exe)
        .args(&args)
        .status()
        .with_context(|| format!("无法执行程序: {}", temp_exe.display()))?;

    // 清理临时文件
    let _ = std::fs::remove_file(&temp_exe);

    // 传递退出码
    if let Some(code) = status.code() {
        std::process::exit(code);
    }

    Ok(())
}

/// 检查命令：只进行类型检查，不生成代码
fn check_command(input: PathBuf, deny_warnings: bool, syntax_only: bool) -> Result<()> {
    println!("✅ 检查: {}", input.display());

    let source_code = std::fs::read_to_string(&input)
        .with_context(|| format!("无法读取文件: {}", input.display()))?;

    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file(
        input.as_os_str().to_str().unwrap().to_string(),
        source_code.clone(),
        &input.clone(),
    );

    // 解析
    let (ast, diagnostics) = parse(&source_map, file_id);

    if !diagnostics.is_empty() {
        eprintln!("❌ 语法错误:");
        for diag in &diagnostics {
            eprintln!("  {}", diag.render(&source_map));
        }
        return Err(anyhow!("语法检查失败"));
    }

    if syntax_only {
        println!("✅ 语法检查通过");
        return Ok(());
    }

    // HIR 转换
    let (hir, diagnostics) = lower(ast);
    if !diagnostics.is_empty() {
        eprintln!("❌ 语法错误:");
        for diag in &diagnostics {
            eprintln!("  {}", diag.render(&source_map));
        }
        return Err(anyhow!("hir生成失败"));
    }

    let resolve_output = resolve(hir, &mut source_map, file_id);

    // 类型检查
    let (_, diagnostics) = check(resolve_output);
    if !diagnostics.is_empty() {
        eprintln!("❌ 类型错误:");
        for diag in &diagnostics {
            eprintln!("  {}", diag.render(&source_map));
        }
        if deny_warnings {
            return Err(anyhow!("类型检查失败"));
        }
    }
    Ok(())
}

/// 清理命令：删除生成的文件
fn clean_command(dir: PathBuf, profile: Option<cli::CleanProfile>) -> Result<()> {
    println!("🧹 清理: {}", dir.display());

    use glob::Pattern;

    let patterns = match profile {
        Some(cli::CleanProfile::Debug) => vec!["*.o", "*.obj", "a.out", "*.exe"],
        Some(cli::CleanProfile::Release) => vec!["*.o", "*.obj", "a.out", "*.exe"],
        Some(cli::CleanProfile::Doc) => vec!["doc/**/*"],
        Some(cli::CleanProfile::All) | None => vec![
            "*.o", "*.obj", // 目标文件
            "*.ll",  // LLVM IR
            "*.s",   // 汇编
            "*.mir", // MIR
            "a.out", "*.exe", // 可执行文件
        ],
    };

    let mut cleaned = 0;

    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();

        if let Some(name) = path.file_name() {
            let name = name.to_string_lossy();
            for pattern in &patterns {
                if Pattern::new(pattern)?.matches(&name) {
                    if path.is_file() {
                        std::fs::remove_file(&path)?;
                        println!("  🗑️  {}", path.display());
                        cleaned += 1;
                    }
                    break;
                }
            }
        }
    }

    println!("✅ 清理完成，删除 {} 个文件", cleaned);
    Ok(())
}

/// 测试命令：运行测试
fn test_command(filter: Option<String>, nocapture: bool, jobs: Option<usize>) -> Result<()> {
    println!("🧪 运行测试");

    if let Some(f) = filter {
        println!("   过滤器: {}", f);
    }

    if nocapture {
        println!("   模式: 输出不捕获");
    }

    if let Some(j) = jobs {
        println!("   并行任务数: {}", j);
    }

    // TODO: 实现测试运行器
    println!("⚠️  测试功能暂未实现");

    Ok(())
}

fn compile_to_mir(
    source_code: &str,
    input_path: &PathBuf,
    verbose: bool,
) -> Result<(MirCrate, SourceMap)> {
    if verbose {
        println!("📖 源代码长度: {} 字节", source_code.len());
    }

    let mut source_map = SourceMap::new();
    let file_id = source_map.add_file(
        input_path.as_os_str().to_str().unwrap().to_string(),
        source_code.to_string(),
        &input_path.clone(),
    );

    // 解析
    if verbose {
        println!("🔍 语法分析...");
    }
    let (ast, diagnostics) = parse(&source_map, file_id);

    if !diagnostics.is_empty() {
        for diag in &diagnostics {
            eprintln!("❌ {}", diag.render(&source_map));
        }
        return Err(anyhow!("语法分析失败"));
    }

    // HIR 转换
    if verbose {
        println!("🔄 HIR 转换...");
    }
    let (hir, lower_diagnostics) = lower(ast);

    if !lower_diagnostics.is_empty() {
        for diag in &lower_diagnostics {
            eprintln!("⚠️  {}", diag.render(&source_map));
        }
    }

    let resolve_output = resolve(hir, &mut source_map, file_id);

    // 类型检查
    if verbose {
        println!("✅ 类型检查...");
    }
    let (typed_crate, diagnostics) = check(resolve_output);

    if !diagnostics.is_empty() {
        eprintln!("❌ 类型错误:");
        for diag in &diagnostics {
            eprintln!("  {}", diag.render(&source_map));
        }
        return Err(anyhow!("类型检查失败"));
    }

    // MIR 生成
    if verbose {
        println!("📋 MIR 生成...");
    }
    let mir = build(typed_crate);

    if verbose {
        println!("   生成 {} 个函数", mir.items.len());
    }

    Ok((mir, source_map))
}
