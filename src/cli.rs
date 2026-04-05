// src/cli.rs

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// LiteC 编译器命令行接口
#[derive(Parser, Debug)]
#[command(
    name = "litec",
    version = env!("CARGO_PKG_VERSION"),
    about = "Lite 语言编译器"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
    /// 显示详细的调试信息
    #[arg(short, long, global = true)]
    pub verbose: bool,
    
    /// 不显示颜色输出
    #[arg(long, global = true)]
    pub no_color: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 编译源文件为可执行文件
    #[command(alias = "c")]
    Build {
        /// 要编译的源文件
        #[arg(value_name = "FILE")]
        input: PathBuf,
        
        /// 输出文件名
        #[arg(short = 'o', long, value_name = "FILE")]
        output: Option<PathBuf>,
        
        /// 优化级别
        #[arg(short = 'O', long, value_enum, default_value = "0")]
        opt_level: OptLevel,
        
        /// 只生成目标文件，不链接
        #[arg(short = 'c', long)]
        compile_only: bool,
        
        /// 指定输出目录
        #[arg(short = 'd', long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
        
        /// 输出 LLVM IR 到文件
        #[arg(long)]
        emit_llvm: bool,
        
        /// 输出 MIR 到文件
        #[arg(long)]
        emit_mir: bool,
        
        /// 添加库搜索路径
        #[arg(short = 'l', long, value_name = "PATH")]
        library_path: Vec<PathBuf>,
        
        /// 链接库
        #[arg(short = 'L', long, value_name = "LIB")]
        library: Vec<String>,
    },
    
    /// 只进行语法分析
    #[command(alias = "p")]
    Parse {
        /// 要分析的源文件
        #[arg(value_name = "FILE")]
        input: PathBuf,
        
        /// 显示 AST
        #[arg(long)]
        ast: bool,
        
        /// 显示 HIR
        #[arg(long)]
        hir: bool,
        
        /// 显示 Token 流
        #[arg(long)]
        tokens: bool,
        
        /// 输出到文件而不是 stdout
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    
    /// 生成并显示中间表示 (MIR)
    #[command(alias = "m")]
    Mir {
        /// 源文件
        #[arg(value_name = "FILE")]
        input: PathBuf,
        
        /// 输出 MIR 到文件
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
        
        /// 显示 Typed HIR
        #[arg(long)]
        typed_hir: bool,
        
        /// 显示优化后的 MIR
        #[arg(long)]
        optimized: bool,
    },
    
    /// 编译并运行程序
    #[command(alias = "r")]
    Run {
        /// 源文件
        #[arg(value_name = "FILE")]
        input: PathBuf,
        
        /// 传递给程序的参数
        #[arg(value_name = "ARGS")]
        args: Vec<String>,
        
        /// 优化级别
        #[arg(short = 'O', long, value_enum, default_value = "none")]
        opt_level: OptLevel,
    },
    
    /// 检查源文件（不生成代码）
    #[command(alias = "chk")]
    Check {
        /// 源文件
        #[arg(value_name = "FILE")]
        input: PathBuf,
        
        /// 将所有警告视为错误
        #[arg(long)]
        deny_warnings: bool,
        
        /// 只检查语法，不进行类型检查
        #[arg(long)]
        syntax_only: bool,
    },
    
    /// 清理生成的文件
    Clean {
        /// 清理目录
        #[arg(default_value = ".", value_name = "DIR")]
        dir: PathBuf,
        
        /// 只清理特定类型的文件
        #[arg(long, value_enum)]
        profile: Option<CleanProfile>,
    },
    
    /// 运行测试
    #[command(alias = "t")]
    Test {
        /// 测试名称过滤器
        #[arg(value_name = "FILTER")]
        filter: Option<String>,
        
        /// 显示测试输出
        #[arg(short, long)]
        nocapture: bool,
        
        /// 并行运行测试的数量
        #[arg(short, long)]
        jobs: Option<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OptLevel {
    /// 无优化
    #[value(name = "0")]
    None,
    /// 基本优化
    #[value(name = "1")]
    Basic,
    /// 更多优化
    #[value(name = "2")]
    Default,
    /// 最大优化
    #[value(name = "3")]
    Aggressive,
    /// 优化大小
    #[value(name = "s")]
    Size,
    /// 优化大小（激进）
    #[value(name = "z")]
    SizeAggressive,
}

impl OptLevel {
    pub fn as_u8(&self) -> u8 {
        match self {
            OptLevel::None => 0,
            OptLevel::Basic => 1,
            OptLevel::Default => 2,
            OptLevel::Aggressive => 3,
            OptLevel::Size => 2, // s -> O2 with size focus
            OptLevel::SizeAggressive => 2, // z -> O2 with aggressive size
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProjectKind {
    /// 可执行程序
    #[value(name = "bin")]
    Binary,
    /// 库
    #[value(name = "lib")]
    Library,
    /// 静态库
    #[value(name = "staticlib")]
    StaticLib,
    /// 动态库
    #[value(name = "dylib")]
    DyLib,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum CleanProfile {
    /// 只清理调试构建
    Debug,
    /// 只清理发布构建
    Release,
    /// 只清理文档
    Doc,
    /// 清理所有
    All,
}

impl Cli {
    /// 解析命令行参数
    pub fn parse_args() -> Self {
        Self::parse()
    }
    
    /// 获取输入文件路径（如果适用）
    pub fn input_file(&self) -> Option<&PathBuf> {
        match &self.command {
            Commands::Build { input, .. } => Some(input),
            Commands::Parse { input, .. } => Some(input),
            Commands::Mir { input, .. } => Some(input),
            Commands::Run { input, .. } => Some(input),
            Commands::Check { input, .. } => Some(input),
            _ => None,
        }
    }
    
    /// 检查是否为 verbose 模式
    pub fn is_verbose(&self) -> bool {
        self.verbose
    }
}

/// 编译配置（从 Build 命令提取）
#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub input: PathBuf,
    pub output: Option<PathBuf>,
    pub opt_level: OptLevel,
    pub compile_only: bool,
    pub out_dir: Option<PathBuf>,
    pub emit_llvm: bool,
    pub emit_mir: bool,
    pub library_paths: Vec<PathBuf>,
    pub libraries: Vec<String>,
}

impl From<&Commands> for Option<BuildConfig> {
    fn from(cmd: &Commands) -> Self {
        match cmd {
            Commands::Build { 
                input,
                output,
                opt_level,
                compile_only,
                out_dir,
                emit_llvm,
                emit_mir,
                library_path,
                library,
            } => Some(BuildConfig {
                input: input.clone(),
                output: output.clone(),
                opt_level: *opt_level,
                compile_only: *compile_only,
                out_dir: out_dir.clone(),
                emit_llvm: *emit_llvm,
                emit_mir: *emit_mir,
                library_paths: library_path.clone(),
                libraries: library.clone(),
            }),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    
    #[test]
    fn verify_cli() {
        Cli::command().debug_assert();
    }
}