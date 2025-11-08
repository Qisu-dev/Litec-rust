// src/cli.rs

use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// LiteC 编译器命令行接口
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 编译源文件
    Build {
        /// 要编译的源文件
        input: PathBuf,
        
        /// 输出文件名
        #[arg(short, long)]
        output: Option<PathBuf>,
        
        /// 优化级别 [0-3]
        #[arg(short, long, default_value = "0")]
        optimization: u8,
        
        /// 显示详细的编译信息
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// 只进行语法分析
    Parse {
        /// 要分析的源文件
        input: PathBuf,
        
        /// 显示 AST
        #[arg(long)]
        ast: bool,
    },
    
    /// 生成中间表示
    Mir {
        /// 源文件
        input: PathBuf,
        
        /// 输出 MIR 到文件
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    
    /// 运行程序 (编译并执行)
    Run {
        /// 源文件
        input: PathBuf,
        
        /// 程序参数
        args: Vec<String>,
    },
}

impl Cli {
    pub fn parse_args() -> Self {
        Self::parse()
    }
}