fn main() {
    // Recompile executable resources when icons change, including in development.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
