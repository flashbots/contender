use alloy::{
    primitives::TxHash,
    rpc::types::trace::geth::PreStateFrame,
    transports::{RpcError, TransportErrorKind},
};
use contender_core::db::DbError;
use std::io;
use thiserror::Error;
use tokio::sync::mpsc;

use crate::block_trace::TxTraceReceipt;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Core(#[from] contender_core::Error),

    #[error("csv writer encountered an error: {0}")]
    CsvWriter(#[from] csv::Error),

    #[error("db error: {0}")]
    Db(#[from] DbError),

    #[error("failed to decode trace frame: {0:?}")]
    DecodePrestateTraceFrame(PreStateFrame),

    #[error("handlebars encountered an error while rendering: {0}")]
    HandlebarsRender(#[from] handlebars::RenderError),

    #[error("invalid run id: {0}")]
    InvalidRunId(u64),

    #[error("io error: {0}")]
    Io(#[from] io::Error),

    #[error("no latency metrics found for method {0}")]
    LatencyMetricsEmpty(String),

    #[error("mpsc failed to send trace receipt: {0:?}")]
    MpscSendTraceReceipt(#[from] mpsc::error::SendError<TxTraceReceipt>),

    #[error("receipt for tx {0} is missing a block number")]
    ReceiptMissingBlockNum(TxHash),

    #[error("rpc error: {0}")]
    Rpc(#[from] RpcError<TransportErrorKind>),

    #[error("run (id={0}) does not exist")]
    RunDoesNotExist(u64),

    #[error("serde_json error: {0}")]
    SerdeJson(#[from] serde_json::Error),
}
