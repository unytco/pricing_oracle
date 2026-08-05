# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Sign zome calls via lair when available: `HolochainConfig` reads `CONDUCTOR_CONFIG` + `LAIR_PASSPHRASE_FILE` (defaulting to the fleet paths) and signs as the oracle's own agent key with no capability grant committed per run; falls back to client signing when lair is unavailable.

### Changed

- Outbound HTTPS runs on rustls instead of native-tls (`reqwest` built with `rustls-tls-native-roots`), which takes `native-tls` and `hyper-tls` out of the dependency tree so `cargo build --release` no longer needs system OpenSSL headers — the dependency that broke the deploy build on hosts with OpenSSL 3.0.2.
- **Operators:** certificates are still verified against the host's CA store (`/etc/ssl/certs`, via `rustls-native-certs`), so the server still needs `ca-certificates` and a CA rotation stays an `apt` refresh — no rebuild. reqwest's `rustls-tls` (a compiled-in `webpki-roots` snapshot) was rejected because it would turn every rotation into a rebuild.
- upgrade holochain_client and holo_hash for Holochain 0.6.2-rc.0
- upgrade holochain_client, holo_hash and zfuel for Holochain 0.7
- **Operators:** Holochain 0.7's keystore builds a vendored OpenSSL (`lair_keystore`'s bundled sqlcipher, not switchable off), so the machine running `cargo build --release` now also needs a C toolchain, `perl` and `make`. The shipped binary links no system OpenSSL, so the deploy target is unaffected.
