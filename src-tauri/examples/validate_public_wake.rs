#[path = "../src/default_model.rs"]
mod default_model;
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/wake-word");
    match default_model::validate(&root) {
        Ok(_) => println!("ACE model package passed its declared release policy and integrity checks. Personal-preview evaluation remains pending; this is not a live accuracy test."),
        Err(error) => {eprintln!("ACE_PUBLIC_WAKE_MODEL_INVALID: {error}"); std::process::exit(1);}
    }
}
