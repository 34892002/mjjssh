fn main() {
    #[cfg(target_os = "windows")]
    println!("cargo:rustc-link-arg=/STACK:8388608");

    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    tauri_build::build()
}
