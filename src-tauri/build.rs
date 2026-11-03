#[path = "src/default_model.rs"]
mod default_model;
fn main() {
    println!("cargo:rerun-if-env-changed=ACE_UPDATER_PUBLIC_KEY");
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("windows-common-controls.manifest");
        println!("cargo:rerun-if-changed=windows-common-controls.manifest");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
    println!("cargo:rerun-if-changed=resources/wake-word");
    if std::env::var("PROFILE").as_deref() == Ok("release") {
        assert!(
            !std::env::var("ACE_UPDATER_PUBLIC_KEY")
                .unwrap_or_default()
                .trim()
                .is_empty(),
            "Release requires ACE_UPDATER_PUBLIC_KEY; never embed the private key"
        );
        default_model::validate(std::path::Path::new("resources/wake-word"))
            .unwrap_or_else(|e| panic!("ACE public wake model release validation failed: {e}"));
    }
    // Personal experimental models stay local and are never implicitly shipped.
    println!("cargo:rerun-if-env-changed=ACE_DEFAULT_WAKE_MODEL");
    let source = (std::env::var("PROFILE").as_deref() == Ok("debug"))
        .then(|| std::env::var_os("ACE_DEFAULT_WAKE_MODEL"))
        .flatten()
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
    // Link the same Common Controls dependency into tests and the application.
    // Avoid a second manifest resource in the application's resource.lib.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        tauri_build::try_build(
            tauri_build::Attributes::new()
                .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
        )
        .expect("Tauri Windows resources must compile");
    } else {
        tauri_build::build();
    }
}
