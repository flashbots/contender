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
    #[error("core error")]
    Core(#[from] contender_core::Error),

    #[error("csv writer error")]
    CsvWriter(#[from] csv::Error),

    #[error("db error")]
    Db(#[from] DbError),

    #[error("failed to decode trace frame: {0:?}")]
    DecodePrestateTraceFrame(PreStateFrame),

    #[error("handlebars encountered an error while rendering")]
    HandlebarsRender(#[from] handlebars::RenderError),

    #[error("invalid run id: {0}")]
    InvalidRunId(u64),

    #[error("io error")]
    Io(#[from] io::Error),

    #[error("no latency metrics found for method {0}")]
    LatencyMetricsEmpty(String),

    #[error("mpsc failed to send trace receipt")]
    MpscSendTraceReceipt(#[from] Box<mpsc::error::SendError<TxTraceReceipt>>),

    #[error("receipt for tx {0} is missing a block number")]
    ReceiptMissingBlockNum(TxHash),

    #[error("rpc error")]
    Rpc(#[from] RpcError<TransportErrorKind>),

    #[error("run (id={0}) does not exist")]
    RunDoesNotExist(u64),

    #[error("no runs found for campaign id {0}")]
    CampaignNotFound(String),

    #[error("serde_json error")]
    SerdeJson(#[from] serde_json::Error),
}
