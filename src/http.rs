use anyhow::{Context, Result};

/// The HTTP client every outbound price and forex fetch runs on.
///
/// `use_rustls_tls()` pins this client to rustls. It is redundant while rustls is
/// the only backend compiled in, and it is deliberately not relied on as a guard:
/// cargo features are additive, so restoring `default-tls` would drag `openssl-sys`
/// back in and still compile fine. `the_dependency_tree_has_no_openssl` catches that.
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

    /// The regression this crate's rustls switch exists to prevent: `openssl-sys`
    /// back in the tree means `cargo build --release` needs system OpenSSL headers
    /// again, which is what broke the deploy build. Only the resolved lockfile can
    /// prove its absence — no amount of TLS configuration in `client()` rules it out.
    #[test]
    fn the_dependency_tree_has_no_openssl() {
        let lockfile = include_str!("../Cargo.lock");
        for crate_name in ["openssl-sys", "native-tls", "hyper-tls"] {
            assert!(
                !lockfile.contains(&format!("name = \"{crate_name}\"")),
                "`{crate_name}` is back in Cargo.lock — the release build needs system \
                 OpenSSL headers again. Check reqwest's `default-tls` feature.",
            );
        }
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
