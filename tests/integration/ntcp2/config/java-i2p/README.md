# Java I2P template provenance

These properties are rendered only into a disposable run root. The template
is pinned to Java I2P 2.12.0 revision `2800040`; the builder records the exact
source checkout and configuration hash in run metadata.

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
