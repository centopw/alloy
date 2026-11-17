use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir = Path::new("apps/alloy/ui/src/lib/generated");
    if !out_dir.exists() {
        fs::create_dir_all(out_dir)?;
    }
    let target = out_dir.join("protocol.ts");
    alloy_protocol::export_typescript_to_file(&target)?;
    println!("Generated TypeScript types at {}", target.display());
    let alphabet = out_dir.join("pairing.ts");
    fs::write(&alphabet, alloy_protocol::pairing_typescript())?;
    println!("Generated pairing alphabet at {}", alphabet.display());
    Ok(())
}
