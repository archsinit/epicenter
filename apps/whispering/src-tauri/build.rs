fn main() {
    println!("cargo:rerun-if-changed=src/indicator_panel.m");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
        let object = out.join("indicator_panel.o");
        let library = out.join("libwhispering_indicator_panel.a");
        let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("aarch64") => "arm64",
            _ => "x86_64",
        };
        let status = std::process::Command::new("xcrun")
            .args([
                "clang",
                "-fobjc-arc",
                "-O2",
                "-mmacosx-version-min=10.15",
                "-arch",
                arch,
                "-c",
                "src/indicator_panel.m",
                "-o",
            ])
            .arg(&object)
            .status()
            .expect("Could not run AppKit host compiler");
        assert!(status.success(), "AppKit companion host compilation failed");
        let status = std::process::Command::new("xcrun")
            .args(["ar", "crs"])
            .arg(&library)
            .arg(&object)
            .status()
            .expect("Could not archive AppKit host");
        assert!(status.success(), "AppKit companion host archive failed");
        println!("cargo:rustc-link-search=native={}", out.display());
        println!("cargo:rustc-link-lib=static=whispering_indicator_panel");
        println!("cargo:rustc-link-lib=framework=AppKit");
    }
    tauri_build::build()
}
