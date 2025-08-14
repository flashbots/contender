# Installation

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
