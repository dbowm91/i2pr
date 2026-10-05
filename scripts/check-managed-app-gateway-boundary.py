#!/usr/bin/env python3
"""Static boundary checks for the Plan 355 daemon app gateway."""

from pathlib import Path


source = Path("crates/i2pr-daemon/src/app_gateway.rs").read_text()
manifest = Path("crates/i2pr-daemon/Cargo.toml").read_text()

if "i2pr-app-proto = { path = \"../i2pr-app-proto\" }" not in manifest:
    raise SystemExit("daemon must own the direct managed-app contract dependency")

for forbidden in (
    "TcpStream::connect",
    "tokio::net::TcpStream",
    "std::net::TcpStream",
    "TcpListener::bind",
    "TcpStream::connect(",
):
    if forbidden in source:
        raise SystemExit(f"gateway contains forbidden host socket operation: {forbidden}")

authorization = source.split("pub(crate) struct AppGatewayAuthorization", 1)[1].split(
    "impl AppGatewayAuthorization", 1
)[0]
if "Serialize" in authorization or "Deserialize" in authorization:
    raise SystemExit("gateway authorization must not implement wire serialization")
if "AppPrincipal" not in authorization or "EffectiveCapabilities" not in authorization:
    raise SystemExit("gateway authorization must bind principal and effective capabilities")
production_source = source.split("#[cfg(test)]", 1)[0]
if "RequestedCapability" in production_source:
    raise SystemExit("gateway authorization must not consume requested capabilities")

if "AppService::ControlScoped => return Err(AppGatewayError::UnsupportedService)" not in source:
    raise SystemExit("control_scoped must remain typed unavailable")
if "state.set_addressbook_handle(self.addressbook.clone())" not in source:
    raise SystemExit("private SAM context must receive canonical SharedAddressBook")
if "drive_private_connection(" not in source or "ManagedAppPrivate" not in Path(
    "crates/i2pr-daemon/src/sam.rs"
).read_text():
    raise SystemExit("gateway must use Plan 354 private managed-app SAM seam")
if "drive_private_connection(" not in Path("crates/i2pr-daemon/src/i2cp.rs").read_text():
    raise SystemExit("gateway must use Plan 354 private I2CP seam")
if "sam_config.enabled = false" not in source or "i2cp_config.enabled = false" not in source:
    raise SystemExit("private gateway contexts must keep listeners disabled")

print("managed app gateway boundary: ok")
