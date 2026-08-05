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

        if let Err(why) = every_openssl_sys_is_vendored(lockfile) {
            panic!("{why}");
        }
    }

    /// `Ok` when every `openssl-sys` package in `lockfile` vendors its own OpenSSL; `Err`
    /// with what went wrong when one does not — or when the lockfile no longer reads
    /// closely enough to tell.
    fn every_openssl_sys_is_vendored(lockfile: &str) -> Result<(), String> {
        let stanzas: Vec<_> = openssl_sys_stanzas(lockfile).collect();
        let entries = openssl_sys_entries(lockfile);

        // The vendoring check below only ever sees the stanzas that parsed, so a layout
        // `openssl_sys_stanzas` misses shrinks what it scans instead of failing — down to
        // nothing, which passes on any lockfile at all. Counting the entries a second,
        // looser way is what turns a stanza that went missing into a failure.
        if stanzas.len() != entries {
            return Err(format!(
                "Cargo.lock has {entries} `openssl-sys` package entries but only {} \
                 `[[package]]` stanza(s) for it parsed — the lockfile's layout changed and \
                 the vendoring check no longer reads every entry.",
                stanzas.len(),
            ));
        }

        if !stanzas.into_iter().all(vendors_its_own_openssl) {
            return Err(
                "`openssl-sys` is in Cargo.lock without `openssl-src`, so it links \
                 the build host's OpenSSL rather than a vendored copy — the release build \
                 needs system OpenSSL headers again."
                    .to_owned(),
            );
        }

        Ok(())
    }

    /// How many packages `lockfile` names `openssl-sys`, read line by line so it shares
    /// none of `openssl_sys_stanzas`' assumptions about stanza layout or line endings.
    fn openssl_sys_entries(lockfile: &str) -> usize {
        lockfile
            .lines()
            .filter(|line| line.trim() == "name = \"openssl-sys\"")
            .count()
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

    /// `openssl-src` as its own package, as a real lockfile carries it — enough on its own
    /// to satisfy a whole-file presence check.
    const OPENSSL_SRC_PREAMBLE: &str =
        "version = 4\n\n[[package]]\nname = \"openssl-src\"\nversion = \"300.6.1\"\n";

    /// One `openssl-sys` `[[package]]` stanza, laid out the way cargo writes it.
    fn openssl_sys_stanza(version: &str, vendored: bool) -> String {
        let openssl_src = if vendored { " \"openssl-src\",\n" } else { "" };
        format!(
            "\n[[package]]\nname = \"openssl-sys\"\nversion = \"{version}\"\n\
             dependencies = [\n \"cc\",\n{openssl_src} \"pkg-config\",\n]\n"
        )
    }

    /// The hole the per-stanza read closes, and the one this crate cannot reproduce
    /// against its own lockfile: cargo re-resolves `Cargo.lock` before it compiles, so an
    /// unvendored `openssl-sys` only ever reaches the check as text.
    #[test]
    fn an_unvendored_openssl_sys_does_not_hide_behind_a_vendored_one() {
        assert!(
            every_openssl_sys_is_vendored(&format!(
                "{OPENSSL_SRC_PREAMBLE}{}{}",
                openssl_sys_stanza("0.9.117", true),
                openssl_sys_stanza("0.10.0", true)
            ))
            .is_ok(),
            "two vendored `openssl-sys` entries both pass — the accept case for a multi-version lockfile",
        );
        assert!(
            every_openssl_sys_is_vendored(&format!(
                "{OPENSSL_SRC_PREAMBLE}{}{}",
                openssl_sys_stanza("0.9.117", true),
                openssl_sys_stanza("0.10.0", false)
            ))
            .is_err(),
            "the second, unvendored `openssl-sys` went unnoticed behind the vendored first",
        );
        assert!(
            every_openssl_sys_is_vendored(&format!(
                "{OPENSSL_SRC_PREAMBLE}{}",
                openssl_sys_stanza("0.9.117", false)
            ))
            .is_err(),
            "a lone unvendored `openssl-sys` went unnoticed next to a stray `openssl-src`",
        );
    }

    /// The hole the entry count closes: `openssl_sys_stanzas` reads one fixed layout, so a
    /// lockfile written any other way narrows what the vendoring check scans rather than
    /// failing, and an unvendored entry rides along in the part that never parsed.
    #[test]
    fn a_lockfile_the_stanza_reader_only_partly_parses_is_rejected() {
        // The same package with `version` written ahead of `name`: an `openssl-sys` entry
        // by any reading, no longer one `openssl_sys_stanzas` matches — and unvendored.
        let unparsed = "\n[[package]]\nversion = \"0.10.0\"\nname = \"openssl-sys\"\n\
                        dependencies = [\n \"cc\",\n \"pkg-config\",\n]\n";
        let partial = format!(
            "{OPENSSL_SRC_PREAMBLE}{}{unparsed}",
            openssl_sys_stanza("0.9.117", true)
        );

        assert!(
            openssl_sys_stanzas(&partial).all(vendors_its_own_openssl),
            "the skipped entry is only a regression case while it stays invisible to the \
             vendoring check — that check now sees it, so this proves nothing",
        );
        assert!(
            every_openssl_sys_is_vendored(&partial).is_err(),
            "an `openssl-sys` entry the reader skipped left the vendoring check passing on \
             the subset it did parse",
        );

        // Nothing parsing at all is the same drift at its limit: every stanza boundary
        // here is `\r\n[[package]]\r\n`, leaving a scan of no stanzas that agrees with any
        // lockfile put to it.
        let crlf = partial.replace('\n', "\r\n");
        assert_eq!(
            openssl_sys_stanzas(&crlf).count(),
            0,
            "a CRLF lockfile is meant to defeat the stanza reader outright here",
        );
        assert!(
            every_openssl_sys_is_vendored(&crlf).is_err(),
            "a lockfile the reader parsed nothing out of passed by default",
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
