use std::{env, fs, path::Path};

use ed25519_dalek::SigningKey;
use i2pr_app_package_build::build_package;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 4 {
        return Err(
            "usage: package <manifest.json> <payload-dir> <output.i2prapp> <raw-32-byte-key>"
                .into(),
        );
    }
    let manifest = fs::read(&args[0])?;
    let key = fs::read(&args[3])?;
    let key: [u8; 32] = key
        .try_into()
        .map_err(|_| "key file must contain exactly 32 bytes")?;
    build_package(
        &manifest,
        Path::new(&args[1]),
        Path::new(&args[2]),
        &SigningKey::from_bytes(&key),
    )?;
    Ok(())
}
