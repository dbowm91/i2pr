use std::path::Path;

use i2pr_daemon::router_i2np::generate_controlled_identity_with_keys;
use i2pr_storage::{IdentityStore, Ssu2TransportIdentityStore};
use rand_core::OsRng;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let data_dir = args.next().ok_or("missing data directory")?;
    let port: u16 = args.next().ok_or("missing SSU2 port")?.parse()?;
    let output = args.next().ok_or("missing output path")?;
    if args.next().is_some() || port < 1024 {
        return Err("invalid arguments".into());
    }

    let data_dir = Path::new(&data_dir);
    let bundle = IdentityStore::in_data_dir(data_dir).load()?;
    let router_hash = bundle.identity().hash()?;
    let transport = Ssu2TransportIdentityStore::in_data_dir(data_dir)
        .load_or_generate(*router_hash.as_bytes(), &mut OsRng)?;
    let identity = generate_controlled_identity_with_keys(&bundle, "127.0.0.1", port, &transport)?;
    std::fs::write(output, identity.router_info)?;
    Ok(())
}
