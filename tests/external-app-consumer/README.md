# External managed-app consumer fixture

This crate intentionally sits outside the router workspace and depends only
on the public SDK, public package builder, and the application's selected
runtime/crypto implementation. It is a git-revision consumer fixture before
the first registry publication.

Build it with `cargo build --manifest-path
tests/external-app-consumer/Cargo.toml --locked`. The app binary speaks the
managed-app protocol over inherited standard streams and completes either the
granted SAM greeting/session or I2CP GetDate/SetDate exchange. The package
builder binary demonstrates signing an artifact without importing the
router's mutable package store or manager protocol. The persistent-catalog
qualification can launch this binary through the real appd/apphost path by
setting `I2PR_EXTERNAL_APP_BINARY` to its built executable.
