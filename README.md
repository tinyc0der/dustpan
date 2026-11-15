# dustpan

dustpan is a disk cleaner with a terminal UI and a scriptable CLI. It aims to cover what [Mole](https://github.com/tw93/Mole) does, on macOS first, then Windows and Linux. It is written in Rust and is an original implementation.

dustpan is pre-release. The only working command so far is `dustpan --version`. Nothing in it deletes files yet.

## Build

You need the Rust toolchain that `rust-toolchain.toml` pins, which `rustup` installs on first use.

```sh
cargo build --release
target/release/dustpan --version
```

Run `cargo test --workspace` for the tests. The plan and the milestone checklist are in `docs/`.

## License

GPL-3.0-or-later. See `LICENSE`.
