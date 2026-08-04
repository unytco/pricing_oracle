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

        let stanzas: Vec<_> = openssl_sys_stanzas(lockfile).collect();

        // Both readers must agree that `openssl-sys` is here, or the strict one below is
        // scanning nothing and would pass on any lockfile at all.
        assert!(
            !has("openssl-sys") || !stanzas.is_empty(),
            "Cargo.lock names `openssl-sys` but no `[[package]]` stanza for it parsed — \
             the lockfile's layout changed and the vendoring check no longer reads it.",
        );

        assert!(
            stanzas.into_iter().all(vendors_its_own_openssl),
            "`openssl-sys` is in Cargo.lock without `openssl-src`, so it links the build \
             host's OpenSSL rather than a vendored copy — the release build needs system \
             OpenSSL headers again.",
        );
    }

    /// Every `openssl-sys` `[[package]]` stanza in `lockfile`, minus its `[[package]]` line.
    fn openssl_sys_stanzas(lockfile: &str) -> impl Iterator<Item = &str> {
        lockfile
            .split("\n[[package]]\n")
            .filter(|stanza| stanza.starts_with("name = \"openssl-sys\"\n"))
    }

    /// Whether one package stanza lists `openssl-src` among *its own* dependencies.
    ///
    /// Read per stanza rather than as a whole-file presence check: an `openssl-src`
    /// anywhere in the lockfile says nothing about which `openssl-sys` pulled it in, so a
    /// second, unvendored entry would otherwise hide behind the vendored one.
    fn vendors_its_own_openssl(stanza: &str) -> bool {
        stanza
            .lines()
            .skip_while(|line| *line != "dependencies = [")
            .take_while(|line| *line != "]")
            .any(|dep| {
                // A dependency reads `"<name>"`, widening to `"<name> <version>"` only
                // once the lockfile carries more than one version of it.
                let dep = dep.trim().trim_end_matches(',');
                dep == "\"openssl-src\"" || dep.starts_with("\"openssl-src ")
            })
    }

    /// The hole the per-stanza read closes, and the one this crate cannot reproduce
    /// against its own lockfile: cargo re-resolves `Cargo.lock` before it compiles, so an
    /// unvendored `openssl-sys` only ever reaches the check as text.
    #[test]
    fn an_unvendored_openssl_sys_does_not_hide_behind_a_vendored_one() {
        let every_openssl_sys_is_vendored =
            |lockfile: &str| openssl_sys_stanzas(lockfile).all(vendors_its_own_openssl);
        let stanza = |version: &str, vendored: bool| {
            let openssl_src = if vendored { " \"openssl-src\",\n" } else { "" };
            format!(
                "\n[[package]]\nname = \"openssl-sys\"\nversion = \"{version}\"\n\
                 dependencies = [\n \"cc\",\n{openssl_src} \"pkg-config\",\n]\n"
            )
        };
        // `openssl-src` is its own package here, as it is in a real lockfile — enough on
        // its own to satisfy a whole-file presence check.
        let preamble =
            "version = 4\n\n[[package]]\nname = \"openssl-src\"\nversion = \"300.6.1\"\n";

        assert!(
            every_openssl_sys_is_vendored(&format!(
                "{preamble}{}{}",
                stanza("0.9.117", true),
                stanza("0.10.0", true)
            )),
            "two vendored `openssl-sys` entries are the state the real lockfile is in",
        );
        assert!(
            !every_openssl_sys_is_vendored(&format!(
                "{preamble}{}{}",
                stanza("0.9.117", true),
                stanza("0.10.0", false)
            )),
            "the second, unvendored `openssl-sys` went unnoticed behind the vendored first",
        );
        assert!(
            !every_openssl_sys_is_vendored(&format!("{preamble}{}", stanza("0.9.117", false))),
            "a lone unvendored `openssl-sys` went unnoticed next to a stray `openssl-src`",
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
