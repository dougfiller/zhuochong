fn main() {
    // macOS: 链接 ApplicationServices 框架（AXIsProcessTrustedWithOptions 需要）
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=ApplicationServices");
        println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=10.13");
    }

    tauri_build::build()
}
