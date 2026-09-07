# XMRig release bundler

Run `cargo build --release` to produce `target/release/xmrig`, or `cargo run`
to build and extract immediately.

To cross-compile Windows x64 from macOS, install the MinGW tools and Rust target:

```sh
brew install mingw-w64
rustup target add x86_64-pc-windows-gnu
cargo build --release --target x86_64-pc-windows-gnu
```

The project configures the MinGW linker in `.cargo/config.toml`. Ensure Homebrew's
`bin` directory is on `PATH`. The result is
`target/x86_64-pc-windows-gnu/release/xmrig.exe`, containing the Windows x64 release.

Each Cargo build checks GitHub's latest stable XMRig release. The build script
selects the official archive for the target OS and architecture, verifies its
SHA-256 against the release's `SHA256SUMS`, and caches it in Cargo's `OUT_DIR`
under `target/`. An unchanged archive is reused. Checking the release still
requires internet access; network and checksum failures fail the build.

The compressed archive is embedded in the executable. Running that executable
extracts the release's contents into `%TEMP%\xmrig` on Windows
and `~/xmrig` on other platforms, retaining executable permissions. The Windows
`TEMP` directory is created if needed. Runtime extraction works offline
and does not start the miner. Each run deletes the entire existing extraction
directory before extraction, including custom configuration and added
files, then installs a fresh copy of the embedded release.

The project's `xmrig/config.json` is also embedded at compile time. After
extraction it replaces the release's default `config.json`. Pool `pass` values
equal to `anon` are replaced with the current OS account's username at runtime;
the wallet address in `user` and other settings remain unchanged. Rebuild after
editing the custom config to embed your changes.

Windows and macOS configs set `pause-on-active` to `true`: mining pauses during
user activity and resumes after 60 seconds idle. Linux and FreeBSD configs omit
this setting because XMRig does not support it on those platforms.
Only macOS removes `cuda`; other platforms retain the template's CUDA setting.

Windows builds embed `xmrig/windows.bat` and copy it alongside the config.
Other platforms include no custom run script. The batch file is copied unchanged
and is not launched automatically.

Supported targets: macOS x64/ARM64, Windows x64/ARM64, Linux x64, and FreeBSD x64.
Windows releases use ZIP; the other platforms use tar.gz. Unsupported targets
fail with an explicit error instead of bundling an incompatible executable.

For a small distributable binary, use `cargo build --release`. The release
profile enables size optimization, LTO, one codegen unit, symbol stripping,
and aborting on panic (no panic unwinding). Plain `cargo build` keeps debug
information for development.

Dependency defaults are disabled. `flate2` enables only `rust_backend`;
Windows-only `zip` enables `deflate-flate2` and shares that backend. `tar`
is included only on non-Windows targets. The build script needs Reqwest's
`blocking`, `json`, and `rustls-tls` features and Serde's `derive` feature.
Build dependencies do not become part of the shipped executable. Most of its
size comes from the embedded, already compressed XMRig release archive.

`cargo test` checks extraction of the actual embedded release into a temporary
directory, replacement of existing files and folders, Unix executable permissions, and refusal
of a symlink destination. It does not write to your real `~/xmrig` directory.
