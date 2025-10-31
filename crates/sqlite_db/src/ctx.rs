use crate::SqliteDb;
use contender_core::generator::{templater::Templater, PlanConfig, RandSeed};
use contender_core::Result;

pub type SqliteCtxBuilder<P> =
    contender_core::orchestrator::ContenderCtxBuilder<SqliteDb, RandSeed, P>;

fn sqlite_ctx<P: PlanConfig<String> + Templater<String> + Send + Sync + Clone>(
    config: P,
    db: SqliteDb,
    rpc: impl AsRef<str>,
) -> SqliteCtxBuilder<P> {
    let seed = RandSeed::new();
    contender_core::ContenderCtx::builder(config, db, seed, rpc)
}

pub fn ctx_builder_filedb<P: PlanConfig<String> + Templater<String> + Send + Sync + Clone>(
    config: P,
    filename: impl AsRef<str>,
    rpc: impl AsRef<str>,
) -> Result<SqliteCtxBuilder<P>> {
    let db = SqliteDb::from_file(filename.as_ref())?;
    Ok(sqlite_ctx(config, db, rpc))
}

pub fn ctx_builder_memdb<P: PlanConfig<String> + Templater<String> + Send + Sync + Clone>(
    config: P,
    rpc: impl AsRef<str>,
) -> SqliteCtxBuilder<P> {
    let db = SqliteDb::new_memory();
    sqlite_ctx(config, db, rpc)
}
