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
};

#[derive(Parser, Debug)]
#[command(
    name = "unarc-sign",
    about = "Sign Unarc release artifacts with Ed25519"
)]
struct Args {
    /// Path to release artifact file (executable or tar.gz)
    #[arg(long)]
    artifact: Option<PathBuf>,

    /// Target operating system (e.g. macos, linux)
    #[arg(long)]
    os: Option<String>,

    /// Target architecture (e.g. aarch64, arm64, x86_64)
    #[arg(long)]
    arch: Option<String>,

    /// Release version (e.g. 0.2.0)
    #[arg(long)]
    version: Option<String>,

    /// Output path for signed manifest.json
    #[arg(long)]
    out_manifest: Option<PathBuf>,

    /// Optional relative or absolute artifact download URL to record in manifest
    #[arg(long)]
    artifact_url: Option<String>,

    /// Expected bundled 7zz SHA-256 hash
    #[arg(long)]
    bundled_7zz_sha256: Option<String>,

    /// Strict release mode: requires explicit valid RELEASE_SIGNING_KEY secret
    #[arg(long)]
    strict: bool,

    /// Verify an existing manifest file against the official embedded trust anchor
    #[arg(long)]
    verify_manifest: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Verification-only mode: verifies manifest signature and (if present) artifact hash
    if let Some(ref manifest_path) = args.verify_manifest {
        println!("Verifying release manifest: {}", manifest_path.display());
        let content = fs::read_to_string(manifest_path)?;
        let manifest: ReleaseManifest = serde_json::from_str(&content)?;

        let verifier = ReleaseSignatureVerifier::official();
        manifest.verify_signature(&verifier)?;
        println!("  -> Ed25519 signature: VALID (verified with official embedded trust anchor)");
        println!("  -> Manifest Version:  {}", manifest.version);
        println!(
            "  -> Manifest Target:   {}-{}",
            manifest.target_os, manifest.target_arch
        );
        println!("  -> Bundled 7zz Hash:  {}", manifest.bundled_7zz_sha256);
        println!("  -> Artifact Hash:     {}", manifest.artifact_sha256);

        let parent = manifest_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let artifact_path = parent.join(&manifest.artifact_url);
        if artifact_path.exists() {
            let actual_hash = compute_sha256(&artifact_path)?;
            if actual_hash != manifest.artifact_sha256 {
                eprintln!(
                    "Error: Artifact SHA-256 mismatch! Manifest: {}, Actual: {}",
                    manifest.artifact_sha256, actual_hash
                );
                std::process::exit(1);
            }
            println!(
                "  -> Artifact File:     {} (SHA-256 MATCH)",
                artifact_path.display()
            );
        }
        println!("Manifest verification SUCCESS.");
        return Ok(());
    }

    let artifact = args.artifact.unwrap_or_else(|| {
        eprintln!("Error: --artifact is required when not in --verify-manifest mode");
        std::process::exit(1);
    });

    if !artifact.exists() {
        eprintln!("Error: Artifact file not found at: {}", artifact.display());
        std::process::exit(1);
    }

    // 1. Resolve signing key from environment
    let signing_key = if let Ok(key_str) = std::env::var("RELEASE_SIGNING_KEY") {
        let key_trimmed = key_str.trim();
        if key_trimmed.is_empty() {
            eprintln!("Error: RELEASE_SIGNING_KEY environment variable is empty");
            std::process::exit(1);
        }
        let bytes = if key_trimmed.len() == 64 {
            hex_decode(key_trimmed).map_err(|e| format!("Invalid RELEASE_SIGNING_KEY hex: {e}"))?
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
    } else {
        eprintln!("Error: RELEASE_SIGNING_KEY environment secret is required to sign manifests");
        std::process::exit(1);
    };

    let os = args.os.unwrap_or_else(|| {
        eprintln!("Error: --os is required when not in --verify-manifest mode");
        std::process::exit(1);
    });
    let arch = args.arch.unwrap_or_else(|| {
        eprintln!("Error: --arch is required when not in --verify-manifest mode");
        std::process::exit(1);
    });
    let version = args.version.unwrap_or_else(|| {
        eprintln!("Error: --version is required when not in --verify-manifest mode");
        std::process::exit(1);
    });
    let out_manifest = args.out_manifest.unwrap_or_else(|| {
        eprintln!("Error: --out-manifest is required when not in --verify-manifest mode");
        std::process::exit(1);
    });

    // 2. Compute SHA-256 of artifact
    let artifact_sha256 = compute_sha256(&artifact)?;
    println!("Artifact: {}", artifact.display());
    println!("Artifact SHA-256: {}", artifact_sha256);

    // 3. Resolve expected bundled 7zz SHA-256
    let bundled_7zz_sha256 = if let Some(hash) = args.bundled_7zz_sha256 {
        hash
    } else {
        match (os.as_str(), arch.as_str()) {
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
                eprintln!("Warning: Unknown OS/Arch combination: {}-{}", os, arch);
                unarc::security::integrity::EXPECTED_ENGINE_BINARY_SHA256_LINUX_X64.to_string()
            }
        }
    };

    let filename = artifact
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("unarc");
    let artifact_url = args.artifact_url.unwrap_or_else(|| filename.to_string());

    // 4. Construct manifest
    let mut manifest = ReleaseManifest {
        version,
        target_os: os,
        target_arch: arch,
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
    fs::write(&out_manifest, manifest_json)?;
    println!("Saved signed manifest to: {}", out_manifest.display());

    // 8. Write accompanying .sha256 checksum file
    let sha_file = artifact.with_extension(format!(
        "{}.sha256",
        artifact.extension().and_then(|e| e.to_str()).unwrap_or("")
    ));
    fs::write(&sha_file, format!("{artifact_sha256}  {filename}\n"))?;
    println!("Saved SHA-256 checksum to: {}", sha_file.display());

    Ok(())
}
