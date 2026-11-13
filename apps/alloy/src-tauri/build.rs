//! Build script for the Tauri shell.
//!
//! `tauri.conf.json` lists `binaries/alloy-meta` as an `externalBin`, so
//! `tauri_build::build()` fails unless `binaries/alloy-meta-<target-triple>` exists (with a
//! `.exe` suffix on Windows). Requiring `just sidecar` before every `cargo build` is a papercut
//! on a fresh checkout, so build the Go sidecar here when it is missing or older than its
//! sources, and do nothing when the binary on disk is already up to date.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Sidecar crate, relative to `CARGO_MANIFEST_DIR` (never to the shell's cwd).
const SIDECAR_DIR: &str = "../../../sidecars/alloy-meta";
/// Directory `tauri.conf.json`'s `externalBin` entry resolves `<name>-<triple>` against.
const BINARIES_DIR: &str = "binaries";
const SIDECAR_NAME: &str = "alloy-meta";
/// Environment that changes the sidecar build output; a change must rerun this script.
const TRACKED_ENV: [&str; 6] = ["TARGET", "GOFLAGS", "CGO_ENABLED", "GOOS", "GOARCH", "GOCACHE"];

fn main() {
    // `option_env!("ALLOY_DISTRIBUTION")` in src/update.rs is read at compile time.
    println!("cargo:rerun-if-env-changed=ALLOY_DISTRIBUTION");
    ensure_sidecar();
    tauri_build::build()
}

/// Materialise `binaries/alloy-meta-<triple>` before `tauri_build::build()` looks for it.
fn ensure_sidecar() {
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR for build scripts"),
    );
    let target = env::var("TARGET").expect("cargo sets TARGET for build scripts");
    let sidecar_dir = manifest_dir.join(SIDECAR_DIR);
    let extension = if target.contains("windows") { ".exe" } else { "" };
    let dest = manifest_dir
        .join(BINARIES_DIR)
        .join(format!("{SIDECAR_NAME}-{target}{extension}"));

    // A directory rerun-if-changed covers every `.go` file (and go.mod/go.sum) below it.
    println!("cargo:rerun-if-changed={}", sidecar_dir.display());
    for var in TRACKED_ENV {
        println!("cargo:rerun-if-env-changed={var}");
    }

    if target.contains("ios") {
        panic!(
            "the Messenger sidecar is out of scope for iOS (decision D10): {target} cannot be \
             cross-built from this build script. Build it on a macOS host with `just sidecar` \
             (GOOS=ios GOARCH=arm64) and place the result at {}",
            dest.display()
        );
    }

    let sources = sidecar_sources(&sidecar_dir);
    if is_up_to_date(&dest, &sources) {
        return;
    }

    if !go_on_path() {
        panic!(
            "\n\
             Cannot build the Messenger sidecar: `go` is not on PATH and no up-to-date binary is \
             present at {}.\n\
             The sidecar (`sidecars/alloy-meta`) is the AGPL-3.0 Go executable that talks NDJSON \
             over stdio to `crates/alloy-sidecar`; every alloy-app build bundles it, and it is \
             never linked into the Rust code.\n\
             Two ways forward:\n\
             1. Install Go 1.26+ (https://go.dev/dl/), make sure `go` is on PATH, and rebuild — \
             this build script then compiles the sidecar automatically.\n\
             2. On a machine that has Go, run `just sidecar` for the host triple or \
             `just sidecar-android` for the arm64 Android library, then copy the resulting \
             `binaries/alloy-meta-*` file into {}.\n",
            dest.display(),
            dest.parent().map_or_else(
                || manifest_dir.join(BINARIES_DIR).display().to_string(),
                |parent| parent.display().to_string()
            )
        );
    }

    build_sidecar(&sidecar_dir, &dest, &target);
}

/// Every file under `dir` whose contents feed `go build` (`*.go`, `go.mod`, `go.sum`).
fn sidecar_sources(dir: &Path) -> Vec<PathBuf> {
    let mut sources = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        let entries = fs::read_dir(&current).unwrap_or_else(|error| {
            panic!(
                "cannot read the Messenger sidecar sources at {} ({error}); `{SIDECAR_DIR}` is \
                 part of this repository and must sit next to `apps/`",
                current.display()
            )
        });
        for entry in entries {
            let entry = entry
                .unwrap_or_else(|error| panic!("cannot read an entry of {}: {error}", current.display()));
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                pending.push(path);
                continue;
            }
            let tracked = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| matches!(extension, "go" | "mod" | "sum"));
            if tracked {
                sources.push(path);
            }
        }
    }
    sources
}

/// True when `dest` exists and is at least as new as every source file.
fn is_up_to_date(dest: &Path, sources: &[PathBuf]) -> bool {
    let Ok(dest_modified) = fs::metadata(dest).and_then(|meta| meta.modified()) else {
        return false;
    };
    sources.iter().all(|source| {
        fs::metadata(source)
            .and_then(|meta| meta.modified())
            .is_ok_and(|source_modified| source_modified <= dest_modified)
    })
}

fn go_on_path() -> bool {
    Command::new("go")
        .arg("version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn build_sidecar(sidecar_dir: &Path, dest: &Path, target: &str) {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .unwrap_or_else(|error| panic!("cannot create {}: {error}", parent.display()));
    }
    println!(
        "cargo:warning=building the Messenger sidecar with Go for {target} -> {}",
        dest.display()
    );

    let mut command = Command::new("go");
    command
        .current_dir(sidecar_dir)
        .env("CGO_ENABLED", "0")
        .args(["build", "-trimpath", "-ldflags", "-s -w", "-o"])
        .arg(dest)
        .arg(".");
    if let Some(goarch) = android_goarch(target) {
        command.env("GOOS", "android").env("GOARCH", goarch);
    }

    let output = command.output().unwrap_or_else(|error| {
        panic!(
            "failed to run `go build` in {}: {error}",
            sidecar_dir.display()
        )
    });
    if !output.status.success() {
        panic!(
            "`go build` for the Messenger sidecar failed in {} ({}):\n--- stdout ---\n{}\n--- stderr ---\n{}",
            sidecar_dir.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
    if !dest.is_file() {
        panic!(
            "`go build` reported success but {} does not exist",
            dest.display()
        );
    }
}

/// Go's `GOARCH` for an Android Rust target; `None` for every non-Android target.
fn android_goarch(target: &str) -> Option<&'static str> {
    if !target.contains("android") {
        return None;
    }
    let goarch = if target.starts_with("aarch64") {
        "arm64"
    } else if target.starts_with("x86_64") {
        "amd64"
    } else if target.starts_with("armv7") {
        "arm"
    } else if target.starts_with("i686") {
        "386"
    } else {
        panic!("unsupported Android target for the Messenger sidecar: {target}")
    };
    Some(goarch)
}
