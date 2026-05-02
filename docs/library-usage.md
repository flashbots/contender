# Using Contender as a library


Add crates you need (using the new per-crate tag scheme):
```toml
[dependencies]
# Use the new tag format: <crate>_v<version>
contender_core   = { git = "https://github.com/flashbots/contender", tag = "contender_core_v0.11.0" }
contender_sqlite = { git = "https://github.com/flashbots/contender", tag = "contender_sqlite_v0.10.0" }
contender_testfile = { git = "https://github.com/flashbots/contender", tag = "contender_testfile_v0.10.1" }

# recommended
tokio = { version = "1.40.0", features = ["rt-multi-thread"] }
```

> **Note:**
> Each crate is now versioned and tagged individually. Use the tag format `<crate>_v<version>` for the crate you want to depend on. For example, to use version 0.10.1 of `contender_core`, use `tag = "contender_core_v0.10.1"`.

Example of contender being used as a library: [op-interop-contender](https://github.com/zeroxbrock/op-interop-contender)
