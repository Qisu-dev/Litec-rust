use anyhow::{Context, Result, anyhow};
use std::path::Path;
use std::process::Command;
use which::which;

pub struct Linker {
    platform: Platform,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Platform {
    Windows,
    Linux,
    MacOS,
    Unknown,
}

impl Linker {
    pub fn new() -> Result<Self> {
        let platform = detect_platform();
        println!("🎯 检测到平台: {:?}", platform);
        Ok(Self { platform })
    }

    pub fn link_executable(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        println!("🔗 链接: {:?} -> {:?}", object_file, output_exe);

        if !object_file.exists() {
            return Err(anyhow!("目标文件不存在"));
        }

        // 检查目标文件类型
        self.check_object_file(object_file)?;

        // 方案1: 使用 LLVM/Clang 链接（推荐，因为目标文件是 LLVM 生成的）
        if let Ok(()) = self.link_with_clang(object_file, output_exe) {
            return Ok(());
        }

        // 方案2: 使用 MSVC link（如果安装了 Visual Studio）
        if let Ok(()) = self.link_with_msvc(object_file, output_exe) {
            return Ok(());
        }

        // 方案3: 使用 LLD（LLVM 链接器）
        if let Ok(()) = self.link_with_lld(object_file, output_exe) {
            return Ok(());
        }

        // 方案4: 尝试 MinGW GCC（可能需要额外的兼容处理）
        if let Ok(()) = self.link_with_mingw(object_file, output_exe) {
            return Ok(());
        }

        Err(anyhow!("所有链接器都失败了"))
    }

    fn check_object_file(&self, object_file: &Path) -> Result<()> {
        println!("📄 检查目标文件...");
        
        // 使用 llvm-objdump 或 objdump 查看文件头
        let dumpers = ["llvm-objdump", "objdump"];
        for dumper in &dumpers {
            if let Ok(output) = Command::new(dumper).arg("-f").arg(object_file).output() {
                let info = String::from_utf8_lossy(&output.stdout);
                println!("   文件信息: {}", info.lines().next().unwrap_or("未知").trim());
                break;
            }
        }

        Ok(())
    }

    /// 使用 Clang 链接（最佳选择，因为目标文件是 LLVM 生成的）
    fn link_with_clang(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        // 尝试不同变体的 clang
        let clang_variants = [
            "clang",
            "clang++", 
            "x86_64-w64-mingw32-clang",
            "i686-w64-mingw32-clang",
        ];

        for clang in &clang_variants {
            if which(clang).is_err() {
                continue;
            }

            let mut cmd = Command::new(clang);
            cmd.arg("-o").arg(output_exe).arg(object_file);

            // Windows 特定选项
            if self.platform == Platform::Windows {
                cmd.arg("-target").arg("x86_64-pc-windows-gnu"); // 或 msvc
                cmd.arg("-fuse-ld=lld"); // 使用 LLD 链接器，更快更兼容
            }

            // 链接 C 库
            cmd.arg("-lc");

            println!("  尝试: {} {:?}", clang, cmd.get_args().collect::<Vec<_>>());

            match cmd.output() {
                Ok(output) if output.status.success() => {
                    println!("✅ 使用 {} 链接成功", clang);
                    return Ok(());
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("⚠️  {} 失败: {}", clang, stderr.lines().next().unwrap_or("未知错误"));
                }
                Err(e) => {
                    println!("⚠️  无法执行 {}: {}", clang, e);
                }
            }
        }

        Err(anyhow!("Clang 链接失败"))
    }

    /// 使用 MSVC link（需要安装 Visual Studio）
    fn link_with_msvc(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        if which("link").is_err() {
            return Err(anyhow!("MSVC link 不存在"));
        }

        let mut cmd = Command::new("link");
        cmd.arg("/NOLOGO")
            .arg("/ENTRY:main")
            .arg("/SUBSYSTEM:CONSOLE")
            .arg(format!("/OUT:{}", output_exe.display()));

        // 添加库路径（从环境变量或默认位置）
        if let Ok(lib_paths) = std::env::var("LIB") {
            for path in lib_paths.split(';') {
                if !path.is_empty() {
                    cmd.arg(format!("/LIBPATH:{}", path));
                }
            }
        }

        // 链接必要的库
        cmd.arg("libcmt.lib")      // C 运行时（静态）
            .arg("kernel32.lib")    // Windows API
            .arg("legacy_stdio_definitions.lib") // 兼容旧版 printf
            .arg(object_file);

        println!("  尝试: MSVC link {:?}", cmd.get_args().collect::<Vec<_>>());

        let output = cmd.output()
            .with_context(|| "无法执行 MSVC link")?;

        if output.status.success() {
            println!("✅ 使用 MSVC link 链接成功");
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!("⚠️  MSVC link 失败: {}", stderr);
            Err(anyhow!("MSVC link 失败"))
        }
    }

    /// 使用 LLD（LLVM 链接器）
    fn link_with_lld(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        // LLD 可以作为 GNU ld 或 MSVC link 的替代品
        let lld_variants = [
            ("lld", "gnu"),           // GNU 风格
            ("lld-link", "msvc"),     // MSVC 风格
            ("ld.lld", "gnu"),        // GNU 风格（Linux 常用）
            ("ld64.lld", "darwin"),   // macOS 风格
        ];

        for (lld, style) in &lld_variants {
            if which(lld).is_err() {
                continue;
            }

            let mut cmd = Command::new(lld);

            match *style {
                "msvc" => {
                    // MSVC 风格
                    cmd.arg("/ENTRY:main")
                        .arg("/SUBSYSTEM:CONSOLE")
                        .arg(format!("/OUT:{}", output_exe.display()))
                        .arg("/DEFAULTLIB:libcmt")
                        .arg("/DEFAULTLIB:kernel32")
                        .arg(object_file);
                }
                "gnu" => {
                    // GNU 风格
                    cmd.arg("-o").arg(output_exe)
                        .arg(object_file)
                        .arg("-e").arg("main")
                        .arg("-lkernel32");

                    if self.platform == Platform::Windows {
                        // Windows 下需要指定 C 库
                        cmd.arg("-lmsvcrt");
                    } else {
                        cmd.arg("-lc");
                    }
                }
                _ => continue,
            }

            println!("  尝试: {} ({}) {:?}", lld, style, cmd.get_args().collect::<Vec<_>>());

            match cmd.output() {
                Ok(output) if output.status.success() => {
                    println!("✅ 使用 {} 链接成功", lld);
                    return Ok(());
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("⚠️  {} 失败: {}", lld, stderr.lines().next().unwrap_or("未知错误"));
                }
                Err(e) => {
                    println!("⚠️  无法执行 {}: {}", lld, e);
                }
            }
        }

        Err(anyhow!("LLD 链接失败"))
    }

    /// 使用 MinGW GCC（备选方案）
    fn link_with_mingw(&self, object_file: &Path, output_exe: &Path) -> Result<()> {
        let gcc_variants = ["gcc", "x86_64-w64-mingw32-gcc"];

        for gcc in &gcc_variants {
            if which(gcc).is_err() {
                continue;
            }

            // 方法1: 简单链接
            let mut cmd = Command::new(gcc);
            cmd.arg("-o").arg(output_exe).arg(object_file);

            println!("  尝试: {} (简单) {:?}", gcc, cmd.get_args().collect::<Vec<_>>());

            match cmd.output() {
                Ok(output) if output.status.success() => {
                    println!("✅ 使用 {} 链接成功", gcc);
                    return Ok(());
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("⚠️  {} 简单模式失败: {}", gcc, stderr.lines().next().unwrap_or("未知错误"));
                }
                Err(e) => {
                    println!("⚠️  无法执行 {}: {}", gcc, e);
                }
            }

            // 方法2: 显式指定 LLVM 兼容的 C 库
            // LLVM 生成的代码通常期望链接到 msvcrt.dll，而不是 MinGW 的 C 库
            let mut cmd = Command::new(gcc);
            cmd.arg("-o").arg(output_exe)
                .arg(object_file)
                // 使用 -nostdlib 避免 MinGW 的 C 库
                .arg("-nostdlib")
                // 手动链接到 msvcrt.dll（Windows 原生 C 库）
                .arg("-lmsvcrt")
                .arg("-lkernel32")
                // 添加 GCC 运行时（用于一些内部函数）
                .arg("-lgcc");

            println!("  尝试: {} (nostdlib) {:?}", gcc, cmd.get_args().collect::<Vec<_>>());

            match cmd.output() {
                Ok(output) if output.status.success() => {
                    println!("✅ 使用 {} (nostdlib) 链接成功", gcc);
                    return Ok(());
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    println!("⚠️  {} nostdlib 失败: {}", gcc, stderr);
                }
                Err(e) => {
                    println!("⚠️  无法执行 {} nostdlib: {}", gcc, e);
                }
            }
        }

        Err(anyhow!("MinGW GCC 链接失败"))
    }
}

fn detect_platform() -> Platform {
    match std::env::consts::OS {
        "windows" => Platform::Windows,
        "linux" => Platform::Linux,
        "macos" => Platform::MacOS,
        _ => Platform::Unknown,
    }
}