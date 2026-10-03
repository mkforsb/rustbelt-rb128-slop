//! For web builds, compile the DSP engine as a standalone C-ABI wasm module
//! (it runs inside an AudioWorklet, separate from the UI's wasm-bindgen
//! module) and expose it to the app via `include_bytes!`.

use std::{env, path::PathBuf, process::Command};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let ws = manifest.join("../..").canonicalize().unwrap();
    for p in [
        "crates/engine/src",
        "crates/engine/Cargo.toml",
        "crates/engine-wasm/src",
        "crates/engine-wasm/Cargo.toml",
    ] {
        println!("cargo:rerun-if-changed={}", ws.join(p).display());
    }
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32") {
        return;
    }

    let target_dir = ws.join("target/engine-wasm");
    let mut cmd = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()));
    cmd.current_dir(&ws)
        .args([
            "build",
            "-p",
            "rustbelt-engine-wasm",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
        ])
        .arg("--target-dir")
        .arg(&target_dir);
    // Don't leak the outer build's configuration (dx sets custom linkers,
    // rustflags and wrappers for hot-patching) into the nested build.
    for (k, _) in env::vars() {
        let keep = k == "CARGO_HOME"
            || k.starts_with("CARGO_NET")
            || k.starts_with("CARGO_REGISTR")
            || k == "CARGO_HTTP_TIMEOUT";
        if (k.starts_with("CARGO_") && !keep)
            || matches!(
                k.as_str(),
                "RUSTFLAGS" | "RUSTDOCFLAGS" | "RUSTC_WORKSPACE_WRAPPER" | "RUSTC_WRAPPER"
            )
        {
            cmd.env_remove(k);
        }
    }
    let status = cmd
        .status()
        .expect("failed to run cargo for the engine wasm");
    assert!(status.success(), "building rustbelt-engine-wasm failed");

    let built = target_dir.join("wasm32-unknown-unknown/release/rustbelt_engine_wasm.wasm");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("rustbelt_engine.wasm");
    std::fs::copy(&built, &out).expect("copy engine wasm");
}
