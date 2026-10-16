// Build script for hello-plugin guest component
// The plugin is compiled to wasm32-wasip2 and will be componentized by the Alloy host
// during testing.

fn main() {
    // Ensure the WIT files are tracked as build dependencies
    println!("cargo:rerun-if-changed=wit/");
}
