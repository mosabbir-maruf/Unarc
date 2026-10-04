use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::PathBuf;

const EXPECTED_SHA256_MACOS: &str =
    "74b0910e50ea44d9760a57fada2192cfd530ba8bffbe7b47c412a464b796cabf";
const EXPECTED_SHA256_LINUX_ARM64: &str =
    "9a26e7d54bfdae8a8f1750cdb70697547b738c2334aa89f3d3f2c8645c8443fe";
const EXPECTED_SHA256_LINUX_X64: &str =
    "3d52c92deb7e9f1bd059692eefc33f86a144cdc548acc4b6f4c809a4dd7bc369";

fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    let mut s = String::with_capacity(64);
    for b in result {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn expected_sha256_for_target(target: &str) -> &'static str {
    if target.contains("darwin") || target.contains("macos") {
        EXPECTED_SHA256_MACOS
    } else if target.contains("aarch64") || target.contains("arm64") {
        EXPECTED_SHA256_LINUX_ARM64
    } else {
        EXPECTED_SHA256_LINUX_X64
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=UNARC_EMBED_7ZZ_PATH");
    println!("cargo:rerun-if-env-changed=UNARC_REQUIRE_EMBEDDED_7ZZ");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set"));
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));
    let target = env::var("TARGET").unwrap_or_default();
    let profile = env::var("PROFILE").unwrap_or_default();

    let require_embedded = env::var("UNARC_REQUIRE_EMBEDDED_7ZZ")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
        || profile == "release";

    let expected_sha = expected_sha256_for_target(&target);

    let mut candidate_paths = Vec::new();

    // 1. Explicit override environment variable
    if let Ok(env_path) = env::var("UNARC_EMBED_7ZZ_PATH") {
        let p = PathBuf::from(env_path);
        candidate_paths.push((p, true));
    }

    // 2. Target-specific and standard relative build paths
    candidate_paths.push((
        manifest_dir
            .join("target")
            .join(&target)
            .join("release")
            .join("7zz"),
        false,
    ));
    candidate_paths.push((
        manifest_dir.join("target").join("release").join("7zz"),
        false,
    ));
    candidate_paths.push((
        manifest_dir
            .join("target")
            .join(&target)
            .join("debug")
            .join("7zz"),
        false,
    ));
    candidate_paths.push((manifest_dir.join("target").join("7zz"), false));
    candidate_paths.push((manifest_dir.join("bundled").join("7zz"), false));
    candidate_paths.push((manifest_dir.join("assets").join("7zz"), false));

    // 3. Known hermetic container locations
    candidate_paths.push((PathBuf::from("/opt/unarc/bin/7zz"), false));
    candidate_paths.push((PathBuf::from("/usr/local/lib/unarc/bin/7zz"), false));

    let mut found_payload = None;

    for (candidate, is_explicit) in candidate_paths {
        if candidate.is_file() {
            println!("cargo:rerun-if-changed={}", candidate.display());
            match fs::read(&candidate) {
                Ok(bytes) => {
                    let actual_sha = compute_sha256(&bytes);
                    if actual_sha.eq_ignore_ascii_case(expected_sha) {
                        found_payload = Some((candidate, bytes));
                        break;
                    } else if is_explicit {
                        panic!(
                            "Explicit UNARC_EMBED_7ZZ_PATH '{}' hash mismatch: computed {}, expected {}",
                            candidate.display(),
                            actual_sha,
                            expected_sha
                        );
                    }
                }
                Err(e) if is_explicit => {
                    panic!(
                        "Failed to read explicit UNARC_EMBED_7ZZ_PATH '{}': {}",
                        candidate.display(),
                        e
                    );
                }
                _ => {}
            }
        }
    }

    let dest = out_dir.join("embedded_7zz.bin");

    if let Some((path, bytes)) = found_payload {
        fs::write(&dest, bytes).expect("Failed to write embedded_7zz.bin to OUT_DIR");
        println!(
            "cargo:warning=Successfully embedded authentic pinned 7zz engine from: {}",
            path.display()
        );
    } else if require_embedded {
        panic!(
            "Release build requires pinned 7zz engine binary for target '{}' (expected SHA-256: {}). \
             Please set UNARC_EMBED_7ZZ_PATH or place 7zz in target/release/7zz before building.",
            target, expected_sha
        );
    } else {
        // In debug/test builds without a local 7zz, write an empty payload so compilation succeeds
        fs::write(&dest, b"").expect("Failed to write empty embedded_7zz.bin to OUT_DIR");
        println!(
            "cargo:warning=Pinned 7zz engine binary was not found for target '{}'. \
             Embedded engine payload will be empty in this debug/test build.",
            target
        );
    }
}
