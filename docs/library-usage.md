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

## Basic Usage

To get started, you'll need the dependencies mentioned in the top section. From `contender_core`, create a `ContenderCtxBuilder` with `ContenderCtx`.

```rs
use std::{sync::Arc, time::Duration};

use contender_core::{
    CancellationToken, Contender, ContenderCtx, RunOpts,
    generator::{FunctionCallDefinition, RandSeed, types::SpamRequest},
    spammer::{LogCallback, TimedSpammer},
};
use contender_sqlite::SqliteDb;
use contender_testfile::TestConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let seed = RandSeed::new();
    let config = TestConfig::new().with_spam(vec![SpamRequest::new_tx(
        &FunctionCallDefinition::new("{_sender}")
            .with_signature("hello(string x)")
            .with_args(&["my friends"]),
    )]);
    let db = SqliteDb::new_memory();
    let ctx = ContenderCtx::builder(config, db, seed, "http://localhost:8545").build();
    let mut contender = Contender::new(ctx).initialize().await?;
    println!("ready to spam!");

    let spammer = TimedSpammer::new(Duration::from_secs(1));
    let provider = contender.provider().clone();
    let callback = Arc::new(LogCallback::new(Arc::new(provider)));
    let opts = RunOpts::new().name("test").periods(10).txs_per_period(50);
    let cancel_token = CancellationToken::new();
    contender
        .spam(spammer, callback, opts, Some(cancel_token.clone()))
        .await?;

    Ok(())
}

```

---

Example of contender being used as a library (outdated but gives you an idea of how to use advanced features): [op-interop-contender](https://github.com/zeroxbrock/op-interop-contender)
