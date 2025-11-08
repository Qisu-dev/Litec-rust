use std::process::{Command, Stdio};
use std::io::{self, Write};
use anyhow::{Result, Context, anyhow};
use std::path::{Path, PathBuf};

/// 跨平台链接管理器
#[derive(Debug)]
pub struct Linker {
    platform: Platform,
    available_linkers: Vec<LinkerType>,
}

#[derive(Debug, Clone, Copy)]
pub enum Platform {
    Windows,
    Linux,
    MacOS,
    Unknown,
}

#[derive(Debug, Clone)]
pub enum LinkerType {
    MsvcLink,
    LldLink,
    MingwGcc,
    MingwClang,
    Gcc,
    Clang,
    Ld,
}

impl Linker {
    pub fn new() -> Result<Self> {
        let platform = detect_platform();
        let available_linkers = detect_available_linkers(platform);
        
        if available_linkers.is_empty() {
            return Err(anyhow!("未找到可用的链接器。请安装开发工具链。"));
        }
        
        dbg!("🎯 检测到平台: {:?}", platform);
        dbg!("🔧 可用链接器: {:?}", &available_linkers);
        
        Ok(Self {
            platform,
            available_linkers,
        })
    }
    
    /// 自动链接目标文件生成可执行文件
    pub fn link_executable(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        dbg!("🔗 开始链接: {:?} -> {:?}", object_file, output_exe);
        
        // 验证目标文件存在
        if !object_file.exists() {
            return Err(anyhow!("目标文件不存在: {:?}", object_file));
        }
        
        // 尝试所有可用的链接器
        for linker_type in &self.available_linkers {
            match self.try_link_with(linker_type, object_file, output_exe) {
                Ok(()) => {
                    dbg!("✅ 使用 {:?} 链接成功", linker_type);
                    return Ok(());
                }
                Err(e) => {
                    dbg!("⚠️  {:?} 链接失败: {}", linker_type, e);
                    continue;
                }
            }
        }
        
        Err(anyhow!("所有链接器都失败了"))
    }
    
    fn try_link_with(&self, linker_type: &LinkerType, object_file: &Path, output_exe: &Path) -> Result<()> {
        match linker_type {
            LinkerType::MsvcLink => self.link_msvc(object_file, output_exe),
            LinkerType::LldLink => self.link_lld(object_file, output_exe),
            LinkerType::MingwGcc => self.link_mingw_gcc(object_file, output_exe),
            LinkerType::MingwClang => self.link_mingw_clang(object_file, output_exe),
            LinkerType::Gcc => self.link_gcc(object_file, output_exe),
            LinkerType::Clang => self.link_clang(object_file, output_exe),
            LinkerType::Ld => self.link_ld(object_file, output_exe),
        }
    }
    
    // Windows 链接器实现
    fn link_msvc(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        let mut cmd = Command::new("link");
        cmd.arg("/NOLOGO")
           .arg("/ENTRY:main")
           .arg("/SUBSYSTEM:CONSOLE")
           .arg("/OUT:")
           .arg(output_exe)
           .arg(object_file);
        
        self.run_command(cmd, "MSVC link")
    }
    
    fn link_lld(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        let mut cmd = Command::new("lld-link");
        cmd.arg("/ENTRY:main")
           .arg("/SUBSYSTEM:CONSOLE")
           .arg("/OUT:")
           .arg(output_exe)
           .arg(object_file);
        
        self.run_command(cmd, "LLD link")
    }
    
    fn link_mingw_gcc(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        let mut cmd = Command::new("x86_64-w64-mingw32-gcc");
        cmd.arg("-o")
           .arg(output_exe)
           .arg(object_file)
           .arg("-static");  // 静态链接避免依赖
        
        self.run_command(cmd, "MinGW GCC")
    }
    
    fn link_mingw_clang(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        let mut cmd = Command::new("x86_64-w64-mingw32-clang");
        cmd.arg("-o")
           .arg(output_exe)
           .arg(object_file)
           .arg("-static");
        
        self.run_command(cmd, "MinGW Clang")
    }
    
    // Unix-like 系统链接器实现
    fn link_gcc(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        let mut cmd = Command::new("gcc");
        cmd.arg("-o")
           .arg(output_exe)
           .arg(object_file)
           .arg("-static")
           .arg("-nostartfiles");
        
        self.run_command(cmd, "GCC")
    }
    
    fn link_clang(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        let mut cmd = Command::new("clang");
        cmd.arg("-o")
           .arg(output_exe)
           .arg(object_file)
           .arg("-static")
           .arg("-nostartfiles");
        
        self.run_command(cmd, "Clang")
    }
    
    fn link_ld(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        let entry_point = match self.platform {
            Platform::MacOS => "_main",
            _ => "main",
        };
        
        let mut cmd = Command::new("ld");
        cmd.arg("-o")
           .arg(output_exe)
           .arg(object_file)
           .arg("-e")
           .arg(entry_point);
        
        // 平台特定的库
        match self.platform {
            Platform::Linux => {
                cmd.arg("-lc")
                   .arg("-dynamic-linker")
                   .arg("/lib64/ld-linux-x86-64.so.2");
            }
            Platform::MacOS => {
                cmd.arg("-lSystem")
                   .arg("-syslibroot")
                   .arg("/Library/Developer/CommandLineTools/SDKs/MacOSX.sdk");
            }
            _ => {}
        }
        
        self.run_command(cmd, "LD")
    }
    
    fn run_command(&self, mut cmd: Command, name: &str) -> Result<()> {
        dbg!("  尝试: {} {:?}", name, cmd.get_args().collect::<Vec<_>>());
        
        let output = cmd
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .output()
            .with_context(|| format!("无法执行链接器: {}", name))?;
        
        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            if !stdout.is_empty() {
                dbg!("    输出: {}", stdout);
            }
            if !stderr.is_empty() {
                dbg!("    错误: {}", stderr);
            }
            
            Err(anyhow!("链接器 {} 返回非零状态", name))
        }
    }
}

/// 检测当前平台
fn detect_platform() -> Platform {
    match std::env::consts::OS {
        "windows" => Platform::Windows,
        "linux" => Platform::Linux,
        "macos" => Platform::MacOS,
        _ => Platform::Unknown,
    }
}

/// 检测可用的链接器
fn detect_available_linkers(platform: Platform) -> Vec<LinkerType> {
    let mut available = Vec::new();
    
    match platform {
        Platform::Windows => {
            // Windows 链接器优先级
            let windows_linkers = [
                (LinkerType::MsvcLink, "link"),
                (LinkerType::LldLink, "lld-link"),
                (LinkerType::MingwGcc, "x86_64-w64-mingw32-gcc"),
                (LinkerType::MingwClang, "x86_64-w64-mingw32-clang"),
                (LinkerType::Gcc, "gcc"),
                (LinkerType::Clang, "clang"),
            ];
            
            for (linker_type, cmd) in windows_linkers {
                if Command::new(cmd).arg("--version").output().is_ok() {
                    available.push(linker_type);
                }
            }
        }
        Platform::Linux | Platform::MacOS => {
            // Unix-like 链接器优先级
            let unix_linkers = [
                (LinkerType::Gcc, "gcc"),
                (LinkerType::Clang, "clang"),
                (LinkerType::Ld, "ld"),
            ];
            
            for (linker_type, cmd) in unix_linkers {
                if Command::new(cmd).arg("--version").output().is_ok() {
                    available.push(linker_type);
                }
            }
        }
        Platform::Unknown => {
            // 未知平台，尝试所有链接器
            let all_linkers = [
                (LinkerType::Gcc, "gcc"),
                (LinkerType::Clang, "clang"),
                (LinkerType::MsvcLink, "link"),
                (LinkerType::LldLink, "lld-link"),
            ];
            
            for (linker_type, cmd) in all_linkers {
                if Command::new(cmd).arg("--version").output().is_ok() {
                    available.push(linker_type);
                }
            }
        }
    }
    
    available
}