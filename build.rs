fn main() {
    // LLVM 路径
    println!("cargo:rustc-link-search=native=D:/llvm/lib");
    println!("cargo:rustc-link-lib=static=LLVM-C");
    
    // vcpkg 手动路径
    println!("cargo:rustc-link-search=native=D:/vcpkg/installed/x64-windows-static/lib");
    
    // 直接指定完整路径，避免 vcpkg crate 的查找
    println!("cargo:rustc-link-arg=D:/vcpkg/installed/x64-windows-static/lib/libxml2.lib");
    println!("cargo:rustc-link-arg=D:/vcpkg/installed/x64-windows-static/lib/zlib.lib");
    
    // 系统库
    for lib in &["psapi", "shell32", "ole32", "uuid", "advapi32", "ws2_32", "kernel32"] {
        println!("cargo:rustc-link-lib=dylib={}", lib);
    }
}