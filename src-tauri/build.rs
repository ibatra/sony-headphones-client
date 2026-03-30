fn main() {
    // On macOS, compile the Objective-C bridge for IOBluetooth framework
    #[cfg(target_os = "macos")]
    {
        cc::Build::new()
            .file("src/bluetooth/macos_bridge.m")
            .flag("-fno-objc-arc") // Use manual reference counting
            .compile("macos_bluetooth_bridge");

        // Link IOBluetooth and Foundation frameworks
        println!("cargo:rustc-link-lib=framework=IOBluetooth");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }

    tauri_build::build()
}
