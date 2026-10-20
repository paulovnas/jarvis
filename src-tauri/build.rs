fn main() {
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let attributes = if msvc {
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
    } else {
        tauri_build::Attributes::new()
    };
    let attributes = attributes.plugin(
        "pixel-processing",
        tauri_build::InlinedPlugin::new().commands(&["process_pixel_art_image"]),
    );
    tauri_build::try_build(attributes).expect("Tauri build configuration failed");
    if msvc {
        // Native child-webview tests and examples also link Common Controls APIs.
        // Embed the same dependency once through the linker for every target;
        // a resource manifest would only reach the main binary and duplicate it there.
        // CEF's DirectComposition child windows require Windows compatibility on every target.
        println!("cargo:rerun-if-changed=windows-app.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
            concat!(env!("CARGO_MANIFEST_DIR"), "/windows-app.manifest")
        );
        println!("cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
