fn main() {
    // `option_env!("ALLOY_DISTRIBUTION")` in src/update.rs is read at compile time.
    println!("cargo:rerun-if-env-changed=ALLOY_DISTRIBUTION");
    tauri_build::build()
}
