# Using Contender as a library

Add crates you need:
```toml
[dependencies]
contender_core   = { git = "https://github.com/flashbots/contender" }
contender_sqlite = { git = "https://github.com/flashbots/contender" }
contender_testfile = { git = "https://github.com/flashbots/contender" }

# recommended
tokio = { version = "1.40.0", features = ["rt-multi-thread"] }
```

Example of contender being used as a library: [op-interop-contender](https://github.com/zeroxbrock/op-interop-contender)
