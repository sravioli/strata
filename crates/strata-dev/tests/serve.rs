//! `strata-dev serve` prints the commands that register it, and serves its JWKS on 127.0.0.1.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Command, Stdio};

#[test]
fn serve_prints_the_commands_and_serves_the_jwks_on_loopback() {
    let dir = std::env::temp_dir().join(format!("strata-dev-serve-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_strata-dev"))
        .args([
            "--key-file",
            dir.join("key.der").to_str().unwrap(),
            "serve",
            "--port",
            "0",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut first = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    let url = first
        .split_whitespace()
        .skip_while(|w| *w != "--jwks-url")
        .nth(1)
        .unwrap()
        .to_owned();
    assert!(
        first.starts_with("strata admin issuer register --issuer strata-dev"),
        "{first}"
    );
    let addr = url
        .strip_prefix("http://")
        .unwrap()
        .strip_suffix("/jwks.json")
        .unwrap();
    assert!(addr.starts_with("127.0.0.1:"), "{addr}");

    let mut s = TcpStream::connect(addr).unwrap();
    write!(
        s,
        "GET /jwks.json HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut resp = String::new();
    s.read_to_string(&mut resp).unwrap();
    child.kill().unwrap();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(dir);
    assert!(resp.starts_with("HTTP/1.1 200"), "{resp}");
    assert!(
        resp.contains("\"kid\":\"strata-dev-1\"") && resp.contains("\"crv\":\"Ed25519\""),
        "{resp}"
    );
}
