//! Verify downloaded updater artifacts with the same key and library as the installed updater.
use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};
use std::{error::Error, fs, path::Path};

fn bounded_read(path: &Path, max: u64) -> Result<Vec<u8>, Box<dyn Error>> {
    if fs::metadata(path)?.len() > max {
        return Err("Artifact exceeds verification size limit".into());
    }
    Ok(fs::read(path)?)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("Usage: verify_update <installer> <signature>".into());
    }
    let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))?;
    let key = config["plugins"]["updater"]["pubkey"].as_str().ok_or("Missing public key")?;
    let key = String::from_utf8(STANDARD.decode(key)?)?;
    let encoded_signature = bounded_read(Path::new(&args[1]), 65536)?;
    let signature = String::from_utf8(STANDARD.decode(encoded_signature.trim_ascii())?)?;
    let bytes = bounded_read(Path::new(&args[0]), 128 * 1024 * 1024)?;
    PublicKey::decode(&key)?.verify(&bytes, &Signature::decode(&signature)?, true)?;
    println!("Updater signature valid. SHA256: {:x}", Sha256::digest(&bytes));
    Ok(())
}
