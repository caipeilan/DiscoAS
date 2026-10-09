use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    if path.extension().is_some_and(|extension| extension == "md") {
        return;
    }
    if path.is_file() {
        files.push(path.to_path_buf());
    } else if path.is_dir() {
        for entry in std::fs::read_dir(path)
            .expect("build input directory")
            .flatten()
        {
            collect(&entry.path(), files);
        }
    }
}

fn main() {
    // Never package an unverified installer dependency. The runtime verifies
    // this same pinned official archive before it extracts anything.
    let archive = std::fs::read("installer/spicetify/spicetify-2.45.3-windows-x64.zip")
        .expect("bundled Spicetify archive is required");
    assert_eq!(
        format!("{:x}", Sha256::digest(&archive)),
        "5d641d4db9caa3891b6a7cc2eb239667af9bb40252a8b413856f9be1e09ff5c1",
        "bundled Spicetify checksum mismatch"
    );
    // Fingerprint production inputs, including frontend code: a source rebuild
    // changes the identifier even when no Git commit or version bump is made.
    // This is a reproducible build label, not a security checksum.
    let inputs = [
        "src",
        "resources",
        "icons",
        "../src",
        "../../crates/discoas-core/src",
        "../../extensions/spotify",
        "../../extensions/browser",
        "installer",
        "../../assets",
        "../package.json",
        "../package-lock.json",
        "Cargo.toml",
        "Cargo.lock",
        "tauri.conf.json",
        "build.rs",
        "../vite.config.ts",
    ];
    let mut files = Vec::new();
    for input in inputs {
        println!("cargo:rerun-if-changed={input}");
        collect(Path::new(input), &mut files);
    }
    files.sort();
    let mut fingerprint = 0xcbf29ce484222325u64;
    for path in files {
        let bytes = std::fs::read(&path).expect("build input file");
        for byte in path
            .to_string_lossy()
            .replace('\\', "/")
            .bytes()
            .chain(bytes)
        {
            fingerprint ^= byte as u64;
            fingerprint = fingerprint.wrapping_mul(0x100000001b3);
        }
    }
    println!("cargo:rustc-env=DISCOAS_BUILD_ID=local-{fingerprint:016x}");
    tauri_build::build()
}
