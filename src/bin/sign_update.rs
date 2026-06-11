use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use ed25519_dalek::{Signer, SigningKey};
use std::path::PathBuf;

const KEY_ENV: &str = "RL_OVERLAY_RELEASE_SIGNING_KEY_B64";

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_usage();
        return Ok(());
    }

    if args.first().is_some_and(|arg| arg == "print-public") {
        let signing_key = signing_key_from_env()?;
        println!("{}", BASE64.encode(signing_key.verifying_key().as_bytes()));
        return Ok(());
    }

    let asset = value_after(&args, "--asset")
        .map(PathBuf::from)
        .ok_or_else(|| "Missing --asset <path>.".to_string())?;
    let out = value_after(&args, "--out")
        .map(PathBuf::from)
        .ok_or_else(|| "Missing --out <path>.".to_string())?;

    let bytes = std::fs::read(&asset)
        .map_err(|error| format!("Could not read asset {}: {error}", asset.display()))?;
    let signature = signing_key_from_env()?.sign(&bytes);
    let asset_name = asset
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("release-asset");
    let content = format!("{}  {asset_name}\n", BASE64.encode(signature.to_bytes()));

    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not create signature directory: {error}"))?;
    }
    std::fs::write(&out, content)
        .map_err(|error| format!("Could not write signature {}: {error}", out.display()))?;
    println!("Wrote {}", out.display());
    Ok(())
}

fn signing_key_from_env() -> Result<SigningKey, String> {
    let encoded = std::env::var(KEY_ENV)
        .map_err(|_| format!("Missing {KEY_ENV}. Set it to a base64 Ed25519 32-byte seed."))?;
    let bytes = BASE64
        .decode(encoded.trim())
        .map_err(|error| format!("Could not decode {KEY_ENV}: {error}"))?;
    let seed: [u8; 32] = bytes
        .try_into()
        .map_err(|_| format!("{KEY_ENV} must decode to exactly 32 bytes."))?;
    Ok(SigningKey::from_bytes(&seed))
}

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.windows(2).find_map(|pair| {
        if pair[0] == flag {
            Some(pair[1].clone())
        } else {
            None
        }
    })
}

fn print_usage() {
    println!(
        "Usage:\n  sign_update --asset <path> --out <path>\n  sign_update print-public\n\nEnvironment:\n  {KEY_ENV}=base64 Ed25519 32-byte seed"
    );
}
