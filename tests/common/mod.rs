use std::io::Read;
use std::net::{Ipv4Addr, TcpListener};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

fn closed_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("read the bound address")
        .port()
}

/// A run against it reaches its end without calling out.
pub const NOTHING_TO_FETCH: &str = r#"
units: []

forex:
  use_twelve_data: false
  use_coinapi: false
  symbols: []
"#;

/// The run's own directory is its working directory, so a relative `--config`
/// resolves there and the run finds no `.env` of the developer's to inherit.
///
/// Bounded, so a hung run fails its own test instead of stalling the suite.
pub fn run_oracle(dir: &Path, args: &[&str]) -> (ExitStatus, String) {
    let log_path = dir.join("run.log");
    let log = std::fs::File::create(&log_path).expect("create the run log");
    let log_err = log.try_clone().expect("a second handle on the run log");

    let mut oracle = Command::new(env!("CARGO_BIN_EXE_pricing-oracle"));
    oracle
        .args(args)
        .current_dir(dir)
        .env("CONDUCTOR_CONFIG", dir.join("conductor-config.yaml"))
        .env("LAIR_PASSPHRASE_FILE", dir.join("lair-passphrase"))
        .env("HOLOCHAIN_ADMIN_PORT", closed_port().to_string())
        .env("HOLOCHAIN_APP_PORT", closed_port().to_string())
        // Callers assert on the run's `info` lines, so a developer's own
        // filter must not reach the child and empty them.
        .env("RUST_LOG", "info")
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));

    // A run that reaches a price source fails to connect instead of calling a
    // third party, whatever proxy the developer's shell sets.
    let nowhere = format!("http://127.0.0.1:{}", closed_port());
    for proxy in [
        "ALL_PROXY",
        "all_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
    ] {
        oracle.env(proxy, &nowhere);
    }
    oracle.env_remove("NO_PROXY").env_remove("no_proxy");

    let mut child = oracle.spawn().expect("run the oracle");

    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        match child.try_wait().expect("poll the oracle") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                child.kill().expect("kill the hung oracle");
                panic!("the oracle did not exit within 60s");
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    };

    let mut output = String::new();
    std::fs::File::open(&log_path)
        .expect("open the run log")
        .read_to_string(&mut output)
        .expect("read the run log");
    (status, output)
}
