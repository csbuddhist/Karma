fn main() {
    // The console talks to KarmaService on Windows and macOS. The
    // `local-backend` feature keeps the standalone development backend for
    // working on the interface without a service.
    println!("cargo::rustc-check-cfg=cfg(karma_service_backend)");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let local_backend = std::env::var_os("CARGO_FEATURE_LOCAL_BACKEND").is_some();
    if target_os == "windows" || (target_os == "macos" && !local_backend) {
        println!("cargo::rustc-cfg=karma_service_backend");
    }
    tauri_build::build()
}
