fn main() {
    // Personal experimental models stay local and are never implicitly shipped.
    println!("cargo:rerun-if-env-changed=ACE_DEFAULT_WAKE_MODEL");
    let source = std::env::var_os("ACE_DEFAULT_WAKE_MODEL")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            (std::env::var("PROFILE").as_deref() == Ok("debug"))
                .then(|| std::path::PathBuf::from("../.local/default-wake.rpw"))
        });
    let bytes = source.map_or_else(Vec::new, |path| {
        println!("cargo:rerun-if-changed={}", path.display());
        if path.is_file() {
            std::fs::read(path).expect("default wake model must be readable")
        } else if std::env::var_os("ACE_DEFAULT_WAKE_MODEL").is_some() {
            panic!("ACE_DEFAULT_WAKE_MODEL must point to an existing model")
        } else {
            Vec::new()
        }
    });
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(output.join("ace-default-wake.rpw"), bytes).unwrap();
    // Recompile executable resources when icons change, including in development.
    println!("cargo:rerun-if-changed=icons");
    tauri_build::build()
}
