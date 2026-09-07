use std::{env, error::Error, fs, path::PathBuf, time::Duration};

use reqwest::blocking::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is missing")?);
    // An intentionally absent input makes Cargo check for updates on every invocation.
    println!(
        "cargo:rerun-if-changed={}",
        out.join("check-latest-always").display()
    );
    let os = env::var("CARGO_CFG_TARGET_OS")?;
    let arch = env::var("CARGO_CFG_TARGET_ARCH")?;
    let suffix = match (os.as_str(), arch.as_str()) {
        ("macos", "aarch64") => "macos-arm64.tar.gz",
        ("macos", "x86_64") => "macos-x64.tar.gz",
        ("linux", "x86_64") => "linux-static-x64.tar.gz",
        ("freebsd", "x86_64") => "freebsd-static-x64.tar.gz",
        ("windows", "x86_64") => "windows-x64.zip",
        ("windows", "aarch64") => "windows-arm64.zip",
        _ => return Err(format!("No supported official XMRig archive for {os}/{arch}").into()),
    };
    let client = Client::builder()
        .user_agent("xmrig-rust-bundler")
        .timeout(Duration::from_secs(180))
        .build()?;
    let release: Release = client
        .get("https://api.github.com/repos/xmrig/xmrig/releases/latest")
        .send()?
        .error_for_status()?
        .json()?;
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(&format!("-{suffix}")))
        .ok_or_else(|| format!("{} has no archive for {os}/{arch}", release.tag_name))?;
    let sums = release
        .assets
        .iter()
        .find(|asset| asset.name == "SHA256SUMS")
        .ok_or("Release has no SHA256SUMS")?;
    let sums = client
        .get(&sums.browser_download_url)
        .send()?
        .error_for_status()?
        .text()?;
    let expected = sums
        .lines()
        .find_map(|line| {
            let mut parts = line.split_whitespace();
            let hash = parts.next()?;
            (parts.next()?.trim_start_matches('*') == asset.name).then_some(hash)
        })
        .ok_or("Archive missing from SHA256SUMS")?;
    let archive = out.join(format!("xmrig-{suffix}"));
    let valid = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes)) == expected;
    if !fs::read(&archive).is_ok_and(|bytes| valid(&bytes)) {
        let bytes = client
            .get(&asset.browser_download_url)
            .send()?
            .error_for_status()?
            .bytes()?;
        if !valid(&bytes) {
            return Err("XMRig download failed SHA-256 verification".into());
        }
        fs::write(&archive, bytes)?;
    }
    println!("cargo:rustc-env=XMRIG_ARCHIVE={}", archive.display());
    println!(
        "cargo:rustc-env=XMRIG_ARCHIVE_FORMAT={}",
        if suffix.ends_with(".zip") {
            "zip"
        } else {
            "tar.gz"
        }
    );
    println!("cargo:rustc-env=XMRIG_VERSION={}", release.tag_name);
    Ok(())
}
