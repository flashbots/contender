# Installation

## Pre-built Binaries (Recommended)

Download a pre-built binary from the [GitHub Releases](https://github.com/flashbots/contender/releases) page. Binaries are available for Linux (x86_64, aarch64) and macOS (x86_64, aarch64). All release binaries are GPG-signed.

```bash
# Example: download and install the latest linux/amd64 binary
curl -L https://github.com/flashbots/contender/releases/latest/download/contender-x86_64-unknown-linux-gnu.tar.gz | tar xz
sudo mv contender /usr/local/bin/
```

## Build from Source

Prereqs:
- [Rust toolchain](https://www.rust-lang.org/tools/install)
- `libsqlite3-dev` (Linux)

Install the CLI from GitHub:
```bash
cargo install --git https://github.com/flashbots/contender --locked
```

Using repo-provided scenarios? Prefix your `<testfile>` with `scenario:`:

```bash
contender setup scenario:stress.toml
contender spam scenario:stress.toml --tps 20
```
