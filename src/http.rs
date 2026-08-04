use anyhow::{Context, Result};

/// The HTTP client every outbound price and forex fetch runs on.
///
/// `use_rustls_tls()` pins this client to rustls. It is redundant while rustls is
/// the only backend compiled in, and it is deliberately not relied on as a guard:
/// cargo features are additive, so restoring `default-tls` would drag the native-tls
/// stack back in and still compile fine. `the_dependency_tree_has_no_system_openssl`
/// catches that.
pub fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("pricing-oracle/0.1")
        .use_rustls_tls()
        .build()
        .context("building HTTP client")
}

#[cfg(test)]
mod tests {
    use super::client;

    /// The regression this crate's rustls switch exists to prevent: a *system*
    /// OpenSSL dependency means `cargo build --release` needs OpenSSL headers on the
    /// build host again, which is what broke the deploy build. Only the resolved
    /// lockfile can prove its absence — no amount of TLS configuration in `client()`
    /// rules it out.
    ///
    /// `openssl-sys` alone stopped being the signal at Holochain 0.7:
    /// `holochain_keystore` pins `lair_keystore` with
    /// `rusqlite-bundled-sqlcipher-vendored-openssl` and exposes no feature to turn
    /// it off, so the crate is unavoidably in the tree. That path is *vendored* —
    /// `openssl-src` builds OpenSSL from source — so it needs no system headers.
    /// What must never come back is an unvendored `openssl-sys`, or the native-tls
    /// stack behind reqwest's `default-tls`.
    #[test]
    fn the_dependency_tree_has_no_system_openssl() {
        let lockfile = include_str!("../Cargo.lock");
        let has = |crate_name: &str| lockfile.contains(&format!("name = \"{crate_name}\""));

        for crate_name in ["native-tls", "hyper-tls"] {
            assert!(
                !has(crate_name),
                "`{crate_name}` is back in Cargo.lock — reqwest is building a native-tls \
                 stack again. Check its `default-tls` feature.",
            );
        }

        assert!(
            !has("openssl-sys") || has("openssl-src"),
            "`openssl-sys` is in Cargo.lock without `openssl-src`, so it links the build \
             host's OpenSSL rather than a vendored copy — the release build needs system \
             OpenSSL headers again.",
        );
    }

    #[test]
    fn client_builds() {
        // Note this says nothing about the host's CA store: reqwest only fails here
        // when it found certificates and none of them parsed, so an absent store
        // still builds and fails later, at the handshake.
        client().expect("HTTP client should build");
    }

    #[tokio::test]
    #[ignore = "requires network — run with `cargo test -- --ignored`"]
    async fn verifies_a_live_price_source_certificate() {
        let response = client()
            .expect("HTTP client should build")
            .get("https://api.coingecko.com/api/v3/ping")
            .send()
            .await;

        // Any HTTP status means the handshake and certificate verification both
        // succeeded; the endpoint's own rate limiting is not what's under test.
        response.expect("TLS handshake with api.coingecko.com should succeed");
    }
}
