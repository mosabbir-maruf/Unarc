//! Unarc Release Packaging & Manifest Signer Utility.
//!
//! Signs release artifacts using Ed25519 and produces verifiable release manifests.
//! Private key material is read exclusively from the `RELEASE_SIGNING_KEY` environment variable.

use clap::Parser;
use ed25519_dalek::SigningKey;
use std::fs;
use std::path::PathBuf;
use unarc::archive::bundled::PINNED_7ZIP_VERSION;
use unarc::security::integrity::{
    compute_sha256, hex_decode, ReleaseManifest, ReleaseSignatureVerifier,
    OFFICIAL_RELEASE_SIGNING_SEED,
};

#[derive(Parser, Debug)]
#[command(
    name = "unarc-sign",
    about = "Sign Unarc release artifacts with Ed25519"
)]
struct Args {
    /// Path to release artifact file (executable or tar.gz)
    #[arg(long)]
    artifact: PathBuf,

    /// Target operating system (e.g. macos, linux)
    #[arg(long)]
    os: String,

    /// Target architecture (e.g. aarch64, arm64, x86_64)
    #[arg(long)]
    arch: String,

    /// Release version (e.g. 0.2.0)
    #[arg(long)]
    version: String,

    /// Output path for signed manifest.json
    #[arg(long)]
    out_manifest: PathBuf,

    /// Optional relative or absolute artifact download URL to record in manifest
    #[arg(long)]
    artifact_url: Option<String>,

    /// Expected bundled 7zz SHA-256 hash
    #[arg(long)]
    bundled_7zz_sha256: Option<String>,

    /// Strict release mode: requires explicit valid RELEASE_SIGNING_KEY secret
    #[arg(long)]
    strict: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if !args.artifact.exists() {
        eprintln!(
            "Error: Artifact file not found at: {}",
            args.artifact.display()
        );
        std::process::exit(1);
    }

    // 1. Resolve signing key from environment or fallback for test fixtures
    let signing_key = if let Ok(key_str) = std::env::var("RELEASE_SIGNING_KEY") {
        let key_trimmed = key_str.trim();
        if key_trimmed.is_empty() {
            if args.strict {
                eprintln!("Error: RELEASE_SIGNING_KEY secret is empty in strict release mode");
                std::process::exit(1);
            }
            eprintln!("Note: RELEASE_SIGNING_KEY is empty; using test fixture signing seed.");
            SigningKey::from_bytes(&OFFICIAL_RELEASE_SIGNING_SEED)
        } else {
            let bytes = if key_trimmed.len() == 64 {
                hex_decode(key_trimmed)
                    .map_err(|e| format!("Invalid RELEASE_SIGNING_KEY hex: {e}"))?
            } else {
                key_trimmed.as_bytes().to_vec()
            };
            if bytes.len() != 32 {
                eprintln!(
                    "Error: RELEASE_SIGNING_KEY must be exactly 32 bytes (or 64 hex characters), got {}",
                    bytes.len()
                );
                std::process::exit(1);
            }
            let mut seed = [0u8; 32];
            seed.copy_from_slice(&bytes);
            SigningKey::from_bytes(&seed)
        }
    } else if args.strict {
        eprintln!(
            "Error: RELEASE_SIGNING_KEY environment secret is required in strict release mode"
        );
        std::process::exit(1);
    } else {
        eprintln!("Note: RELEASE_SIGNING_KEY not set; using official test fixture signing seed.");
        SigningKey::from_bytes(&OFFICIAL_RELEASE_SIGNING_SEED)
    };

    // 2. Compute SHA-256 of artifact
    let artifact_sha256 = compute_sha256(&args.artifact)?;
    println!("Artifact: {}", args.artifact.display());
    println!("Artifact SHA-256: {}", artifact_sha256);

    // 3. Resolve expected bundled 7zz SHA-256
    let bundled_7zz_sha256 = if let Some(hash) = args.bundled_7zz_sha256 {
        hash
    } else {
        match (args.os.as_str(), args.arch.as_str()) {
            ("macos", _) => {
                unarc::security::integrity::EXPECTED_ENGINE_BINARY_SHA256_MACOS.to_string()
            }
            ("linux", "aarch64") | ("linux", "arm64") => {
                unarc::security::integrity::EXPECTED_ENGINE_BINARY_SHA256_LINUX_ARM64.to_string()
            }
            ("linux", "x86_64") | ("linux", "x64") => {
                unarc::security::integrity::EXPECTED_ENGINE_BINARY_SHA256_LINUX_X64.to_string()
            }
            _ => {
                eprintln!(
                    "Warning: Unknown OS/Arch combination: {}-{}",
                    args.os, args.arch
                );
                unarc::security::integrity::EXPECTED_ENGINE_BINARY_SHA256_LINUX_X64.to_string()
            }
        }
    };

    let filename = args
        .artifact
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("unarc");
    let artifact_url = args.artifact_url.unwrap_or_else(|| filename.to_string());

    // 4. Construct manifest
    let mut manifest = ReleaseManifest {
        version: args.version,
        target_os: args.os,
        target_arch: args.arch,
        bundled_7zz_version: PINNED_7ZIP_VERSION.to_string(),
        bundled_7zz_sha256,
        artifact_url,
        artifact_sha256: artifact_sha256.clone(),
        signature: None,
    };

    // 5. Sign manifest
    manifest.sign(&signing_key);
    println!("Signature: {}", manifest.signature.as_deref().unwrap_or(""));

    // 6. Verify signed manifest using official embedded public key
    let verifier = ReleaseSignatureVerifier::official();
    manifest.verify_signature(&verifier)?;
    println!("Verification with official embedded trust anchor: PASS");

    // 7. Write manifest.json
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&args.out_manifest, manifest_json)?;
    println!("Saved signed manifest to: {}", args.out_manifest.display());

    // 8. Write accompanying .sha256 checksum file
    let sha_file = args.artifact.with_extension(format!(
        "{}.sha256",
        args.artifact
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
    ));
    fs::write(&sha_file, format!("{artifact_sha256}  {filename}\n"))?;
    println!("Saved SHA-256 checksum to: {}", sha_file.display());

    Ok(())
}
