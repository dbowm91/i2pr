# Java I2P template provenance

These properties are rendered only into a disposable run root. The template
is pinned to Java I2P 2.12.0 revision
`2800040deee9bb376567b671ef2e9c34cf3e30b6`; the builder records the exact
source checkout and configuration hash in cache/run metadata.

The pinned source's directory contract is explicit: `i2p.dir.base` is the
read-only copied installation under `run-root/reference-runtime`,
`i2p.dir.config` is `run-root/config`, and `i2p.dir.router` is the writable
`run-root/reference-data` directory. The router writes `router.info` there;
the Java NetDB import directory is `reference-data/netDb` and uses the
`routerInfo-<I2P-base64-identity-hash>.dat` filename convention. The approved
headless launcher is the staged `i2prouter` shell launcher; the builder only
runs a static shell-syntax inspection, while the adapter starts it in the
reference namespace and watches its bounded output for readiness. SIGTERM is
the graceful shutdown path, followed by a bounded kill/join fallback.

Safety-critical names are traced to the pinned upstream router configuration
and build sources before use: `i2np.allowLocal`, `i2np.ntcp.enable`,
`i2np.ntcp.hostname`, `i2np.ntcp.port`, `i2np.ntcp.autoip`,
`i2np.ntcp.autoport`, `i2np.upnp.enable`, `router.reseedDisable`,
`router.updateDisabled`, `router.floodfillParticipant`,
`router.maxParticipatingTunnels`, `router.newsRefreshFrequency`, and the UDP
disable settings. The adapter asserts the rendered values and
the readiness adapter must reject a startup that rewrites them.

The template intentionally does not enable the daemon, reseed, bootstrap,
automatic address discovery, SSU/SSU2, console, client tunnels, or proxy
listeners. RouterInfo import is a disposable adapter operation; it is not
NetDB publication evidence.

The pinned Java source contract for the private network is the
`router.networkID` property read by the router configuration path in
`router/java/src/net/i2p/router/Router.java` at revision
`2800040deee9bb376567b671ef2e9c34cf3e30b6`. Plan 041 renders the reviewed
non-public value `99`; the adapter parses the final property set and rejects a
missing, duplicate, unknown, or public-network value before launch. This is a
configuration contract, not an advertisement or public NetDB identity.
