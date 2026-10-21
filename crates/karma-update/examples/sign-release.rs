//! Release-signing tool for maintainers.
//!
//! ```text
//! sign-release generate-key <key-file>           # once; prints the public key
//! sign-release sign <key-file> <bundle-dir> <version>
//! sign-release verify <bundle-dir>               # against RELEASE_PUBLIC_KEY
//! ```
//!
//! The key file holds the 32-byte Ed25519 seed as hex. Keep it outside the
//! repository and back it up: without it no further release can update
//! existing installations in place.

use std::{env, fs, path::Path, process::ExitCode};

use ed25519_dalek::SigningKey;
use karma_update::{
    MANIFEST_FILE, RELEASE_PUBLIC_KEY, SIGNATURE_FILE, Version, build_manifest, decode_hex,
    encode_hex, sign_manifest, verify_manifest,
};

fn main() -> ExitCode {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let result = match arguments.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["generate-key", key_file] => generate_key(Path::new(key_file)),
        ["sign", key_file, bundle, version] => sign(Path::new(key_file), Path::new(bundle), version),
        ["verify", bundle] => verify(Path::new(bundle)),
        _ => Err("usage: sign-release generate-key <key-file> | sign <key-file> <bundle-dir> <version> | verify <bundle-dir>".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn generate_key(key_file: &Path) -> Result<(), String> {
    if key_file.exists() {
        return Err(format!(
            "{} already exists; refusing to overwrite a signing key",
            key_file.display()
        ));
    }
    let key = SigningKey::generate(&mut rand::rngs::OsRng);
    if let Some(parent) = key_file.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    write_private(key_file, &encode_hex(&key.to_bytes()))?;
    println!("{}", encode_hex(&key.verifying_key().to_bytes()));
    Ok(())
}

#[cfg(unix)]
fn write_private(path: &Path, contents: &str) -> Result<(), String> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut file| file.write_all(contents.as_bytes()))
        .map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn write_private(path: &Path, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|error| error.to_string())
}

fn load_key(key_file: &Path) -> Result<SigningKey, String> {
    let text = fs::read_to_string(key_file).map_err(|error| error.to_string())?;
    let seed: [u8; 32] = decode_hex(text.trim())
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or("signing key file is not a 32-byte hex seed")?;
    Ok(SigningKey::from_bytes(&seed))
}

fn sign(key_file: &Path, bundle: &Path, version: &str) -> Result<(), String> {
    let key = load_key(key_file)?;
    if key.verifying_key().to_bytes() != RELEASE_PUBLIC_KEY {
        return Err("signing key does not match RELEASE_PUBLIC_KEY in karma-update".into());
    }
    let version: Version = version.parse().map_err(|_| "invalid version")?;
    let manifest = build_manifest(bundle, version).map_err(|error| error.to_string())?;
    let mut bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    fs::write(bundle.join(MANIFEST_FILE), &bytes).map_err(|error| error.to_string())?;
    fs::write(
        bundle.join(SIGNATURE_FILE),
        sign_manifest(&bytes, &key) + "\n",
    )
    .map_err(|error| error.to_string())?;
    verify(bundle)?;
    println!("signed {} files for {version}", manifest.files.len());
    Ok(())
}

/// Checks the signature and that every listed file still matches.
fn verify(bundle: &Path) -> Result<(), String> {
    let bytes = fs::read(bundle.join(MANIFEST_FILE)).map_err(|error| error.to_string())?;
    let signature =
        fs::read_to_string(bundle.join(SIGNATURE_FILE)).map_err(|error| error.to_string())?;
    let manifest = verify_manifest(&bytes, &signature, &RELEASE_PUBLIC_KEY)
        .map_err(|error| format!("manifest verification failed: {error}"))?;
    let current = build_manifest(bundle, manifest.version()).map_err(|error| error.to_string())?;
    if current.files != manifest.files {
        return Err("bundle files differ from the signed manifest".into());
    }
    println!("verified {} for {}", MANIFEST_FILE, manifest.version);
    Ok(())
}
