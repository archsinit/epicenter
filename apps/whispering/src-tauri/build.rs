fn main() {
    println!("cargo:rerun-if-changed=src/indicator_panel.m");
    println!("cargo:rerun-if-changed=src/meeting/capture_macos.m");
    println!("cargo:rerun-if-changed=src/meeting/capture_macos.h");
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
        println!("cargo:rustc-link-lib=framework=QuartzCore");

        let object = out.join("meeting_capture.o");
        let library = out.join("libwhispering_meeting_capture.a");
        let status = std::process::Command::new("xcrun")
            .args([
                "clang",
                "-fobjc-arc",
                "-O2",
                "-mmacosx-version-min=10.15",
                "-Werror=unguarded-availability-new",
                "-Werror=incompatible-pointer-types",
                "-arch",
                arch,
                "-c",
                "src/meeting/capture_macos.m",
                "-o",
            ])
            .arg(&object)
            .status()
            .expect("Could not run meeting capture compiler");
        assert!(status.success(), "Meeting capture compilation failed");
        let status = std::process::Command::new("xcrun")
            .args(["ar", "crs"])
            .arg(&library)
            .arg(&object)
            .status()
            .expect("Could not archive meeting capture bridge");
        assert!(status.success(), "Meeting capture archive failed");
        println!("cargo:rustc-link-lib=static=whispering_meeting_capture");
        for framework in ["AVFoundation", "CoreMedia", "CoreGraphics"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        // Keep ordinary dictation launchable below macOS 12.3, where this
        // optional framework does not exist. Meeting APIs are guarded at 14.0.
        println!("cargo:rustc-link-arg=-Wl,-weak_framework,ScreenCaptureKit");
    }
    tauri_build::build()
}
