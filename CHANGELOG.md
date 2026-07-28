# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Sign zome calls via lair when available: `HolochainConfig` reads `CONDUCTOR_CONFIG` + `LAIR_PASSPHRASE_FILE` (defaulting to the fleet paths) and passes them to ham's `try_lair_signing_from_node`, so the oracle signs as its own agent key with no capability grant committed per run; falls back to client signing when lair is unavailable.

### Changed

- Outbound HTTPS runs on rustls instead of native-tls: `reqwest` is built with `default-features = false` and `rustls-tls-native-roots`, which takes `openssl-sys`, `native-tls` and `hyper-tls` out of the dependency tree. `cargo build --release` no longer needs system OpenSSL headers — that dependency is what broke the deploy build on hosts with OpenSSL 3.0.2 (`OPENSSL_API_COMPAT expresses an impossible API compatibility level`). The other reqwest features (`charset`, `http2`, `system-proxy`) are its own former defaults, kept so nothing but the TLS backend changes.
- **Operators:** certificates are still verified against the host's CA store (`/etc/ssl/certs`, read via `rustls-native-certs`) exactly as the OpenSSL backend did, so the server still needs `ca-certificates` present, and keeping it current with `apt` remains all a CA rotation takes. The alternative — reqwest's `rustls-tls`, which compiles in a `webpki-roots` snapshot — was rejected because it would turn every CA rotation into a rebuild and re-deploy of the binary.
- upgrade holochain_client and holo_hash for Holochain 0.6.2-rc.0
