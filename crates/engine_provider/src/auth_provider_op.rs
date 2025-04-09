use std::path::PathBuf;

use alloy::hex::ToHexExt;
use op_rbuilder::tester::{BlockGenerator, EngineApi, EngineApiBuilder};

use crate::{read_jwt_file, AdvanceChain, DEFAULT_BLOCK_TIME};

pub struct AuthProviderOp {
    engine_client: EngineApi,
}
