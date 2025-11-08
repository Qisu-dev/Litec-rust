// src/main.rs

mod cli;

use std::{path::PathBuf, process::Command};

use anyhow::{anyhow, Context, Result};
use cli::{Cli, Commands};
use litec_codegen::{codegen, linker::Linker};
use litec_lower::lower_crate;
use litec_mir_lower::build_mir;
use litec_parse::parser::parse;
use litec_type_checker::check;

fn main() -> Result<()> {
    let cli = Cli::parse_args();
    
    match cli.command {
        Commands::Build { input, output, optimization, verbose } => {
            build_command(input, output, optimization, verbose)
        }
        Commands::Parse { input, ast } => {
            parse_command(input, ast)
        }
        Commands::Mir { input, output } => {
            mir_command(input, output)
        }
        Commands::Run { input, args } => {
            run_command(input, args)
        }
    }
}

fn build_command(
    input: std::path::PathBuf,
    output: Option<std::path::PathBuf>,
    optimization: u8,
    verbose: bool,
) -> Result<()> {
    if verbose {
        println!("🔨 构建模式");
        println!("📄 输入文件: {:?}", input);
        println!("🎯 输出文件: {:?}", output);
        println!("⚡ 优化级别: {}", optimization);
    }
    
    let source_code = std::fs::read_to_string(&input)
        .with_context(|| format!("无法读取文件: {:?}", input))?;
    
    // 调用你的编译器
    compile_source(&source_code, optimization, verbose)?;
    
    if verbose {
        println!("✅ 构建完成!");
    }
    
    Ok(())
}

fn parse_command(input: std::path::PathBuf, ast: bool) -> Result<()> {
    println!("🔍 解析模式: {:?}", input);
    
    let source_code = std::fs::read_to_string(&input)
        .with_context(|| format!("无法读取文件: {:?}", input))?;
    
    // 调用你的解析器
    if ast {
        println!("🌳 显示 AST");
        // 显示抽象语法树
    }
    
    println!("✅ 解析完成");
    Ok(())
}

fn mir_command(input: std::path::PathBuf, output: Option<std::path::PathBuf>) -> Result<()> {
    println!("🔄 MIR 生成模式: {:?}", input);
    
    let source_code = std::fs::read_to_string(&input)
        .with_context(|| format!("无法读取文件: {:?}", input))?;
    
    // 调用你的 MIR 生成器
    if let Some(output_path) = output {
        println!("💾 输出到: {:?}", output_path);
    }
    
    println!("✅ MIR 生成完成");
    Ok(())
}

fn run_command(input: PathBuf, args: Vec<String>) -> Result<()> {
    println!("🏃 运行模式: {:?}", input);
    println!("📝 程序参数: {:?}", args);
    
    // 读取源代码
    let source_code = std::fs::read_to_string(&input)
        .with_context(|| format!("无法读取文件: {:?}", input))?;
    
    println!("📖 源代码长度: {} 字符", source_code.len());
    
    // 解析阶段
    let ast = match parse(&source_code) {
        Ok(ast) => {
            println!("✅ 语法分析成功");
            ast
        },
        Err(errs) => {
            eprintln!("❌ 语法分析失败，错误数: {}", errs.len());
            for err in errs {
                eprintln!("   {}", err);
            }
            return Err(anyhow!("语法分析失败"));
        }
    };

    // HIR 转换
    let hir = match lower_crate(ast) {
        Ok(hir) => {
            println!("✅ HIR转换成功");
            hir
        },
        Err(errs) => {
            eprintln!("❌ HIR转换失败，错误数: {}", errs.len());
            for err in errs {
                eprintln!("   {}", err);
            }
            return Err(anyhow!("HIR转换失败"));
        }
    };

    // 类型检查
    let typed_hir = match check(hir, input.clone()) {
        Ok(typed_hir) => {
            println!("✅ 类型检查成功");
            typed_hir
        },
        Err(errs) => {
            eprintln!("❌ 类型检查失败，错误数: {}", errs.len());
            for err in errs {
                eprintln!("   {}", err);
            }
            return Err(anyhow!("类型检查失败"));
        },
    };

    // 生成 MIR
    let mir = build_mir(&typed_hir);
    println!("📋 生成 MIR，包含 {} 个函数", mir.len());

    // 生成目标文件路径
    let object_file = input.with_extension(if cfg!(windows) { "obj" } else { "o" });
    
    // 代码生成 - 编译到目标文件
    let executable_data = match codegen(mir, input.file_name().unwrap().to_str().unwrap()) {
        Ok(executable) => {
            println!("✅ 代码生成成功，生成 {} 字节目标文件", executable.len());
            executable
        },
        Err(e) => {
            eprintln!("❌ 代码生成失败: {}", e);
            return Err(anyhow!("代码生成失败: {}", e));
        }
    };

    // 写入目标文件
    std::fs::write(&object_file, &executable_data)
        .with_context(|| format!("无法写入目标文件: {:?}", object_file))?;
    println!("💾 目标文件已写入: {:?}", object_file);

    // 使用您现有的链接器链接可执行文件
    let output_exe = input.with_extension(if cfg!(windows) { "exe" } else { "" });
    
    println!("🔗 开始链接过程...");
    let linker = Linker::new()
        .with_context(|| "无法初始化链接器")?;
    
    linker.link_executable(&object_file, &output_exe)
        .with_context(|| format!("链接失败: {:?} -> {:?}", object_file, output_exe))?;
    
    println!("✅ 链接成功: {:?}", output_exe);

    // 清理临时文件
    if let Err(e) = std::fs::remove_file(&object_file) {
        println!("⚠️  无法删除临时文件 {:?}: {}", object_file, e);
    } else {
        println!("🧹 已清理临时文件: {:?}", object_file);
    }

    // 检查文件大小
    if let Ok(metadata) = std::fs::metadata(&output_exe) {
        println!("📦 可执行文件大小: {} 字节", metadata.len());
    }

    // 如果用户提供了 --run 参数，自动运行程序
    let should_run = args.iter().any(|arg| arg == "--run") || args.is_empty();
    
    if should_run {
        println!("🚀 自动运行程序...");
        run_executable(&output_exe, &args)?;
    } else {
        println!("💾 可执行文件已生成: {:?}", output_exe);
        println!("🎉 编译完成");
    }
    
    Ok(())
}

/// 运行生成的可执行文件
fn run_executable(executable_path: &PathBuf, original_args: &[String]) -> Result<()> {
    // 过滤掉 --run 参数
    let run_args: Vec<&String> = original_args
        .iter()
        .filter(|arg| arg != &&"--run".to_string())
        .collect();
    
    println!("▶️  执行: {:?} {:?}", executable_path, run_args);
    
    let status = Command::new(executable_path)
        .args(run_args)
        .status()
        .with_context(|| format!("无法执行程序: {:?}", executable_path))?;
    
    if status.success() {
        println!("✅ 程序执行成功");
    } else {
        println!("⚠️  程序退出状态: {}", status);
    }
    
    Ok(())
}

// 如果您需要处理特定的链接错误，可以添加这个辅助函数
fn handle_linker_error(error: &anyhow::Error) {
    eprintln!("❌ 链接器错误: {}", error);
    
    // 提供有用的调试信息
    #[cfg(target_os = "windows")]
    eprintln!(
        "💡 Windows 链接提示:\n\
         - 确保安装了 Visual Studio Build Tools 或 LLVM\n\
         - 或者安装 MinGW-w64\n\
         - 检查链接器是否在 PATH 环境变量中"
    );
    
    #[cfg(target_os = "linux")]
    eprintln!(
        "💡 Linux 链接提示:\n\
         - 安装 gcc: sudo apt install gcc\n\
         - 或者安装 clang: sudo apt install clang\n\
         - 检查开发工具链是否完整"
    );
    
    #[cfg(target_os = "macos")]
    eprintln!(
        "💡 macOS 链接提示:\n\
         - 安装 Xcode Command Line Tools: xcode-select --install\n\
         - 或者安装 Homebrew 的 llvm: brew install llvm"
    );
}

fn compile_source(source_code: &str, optimization: u8, verbose: bool) -> Result<()> {
    if verbose {
        println!("📋 编译源代码 (长度: {})", source_code.len());
    }
    
    // 这里集成你现有的编译器逻辑
    // 使用你之前实现的各个组件
    
    Ok(())
}