use alloy::rpc::types::TransactionRequest;

/// Wrapper for [`TransactionRequest`](alloy::rpc::types::TransactionRequest) that includes optional name and kind fields.
#[derive(Clone, Debug)]
pub struct NamedTxRequest {
    pub name: Option<String>,
    pub kind: Option<String>,
    pub tx: TransactionRequest,
}

/// Syntactical sugar for creating a [`NamedTxRequest`].
///
/// This is useful for imperatively assigning optional fields to a tx.
/// It is _not_ useful when you're dynamically assigning these fields (i.e. you have an Option to check first).
///
/// ### Example:
/// ```
/// use alloy::rpc::types::TransactionRequest;
/// # use contender_core::generator::NamedTxRequestBuilder;
///
/// let tx_req = TransactionRequest::default();
/// let named_tx_req = NamedTxRequestBuilder::new(tx_req)
///     .with_name("unique_tx_name")
///     .with_kind("tx_kind")
///     .build();
/// assert_eq!(named_tx_req.name, Some("unique_tx_name".to_owned()));
/// assert_eq!(named_tx_req.kind, Some("tx_kind".to_owned()));
/// ```
pub struct NamedTxRequestBuilder {
    name: Option<String>,
    kind: Option<String>,
    tx: TransactionRequest,
}

#[derive(Clone, Debug)]
pub enum ExecutionRequest {
    Tx(NamedTxRequest),
    Bundle(Vec<NamedTxRequest>),
}

impl From<NamedTxRequest> for ExecutionRequest {
    fn from(tx: NamedTxRequest) -> Self {
        Self::Tx(tx)
    }
}

impl From<Vec<NamedTxRequest>> for ExecutionRequest {
    fn from(txs: Vec<NamedTxRequest>) -> Self {
        Self::Bundle(txs)
    }
}

impl NamedTxRequestBuilder {
    pub fn new(tx: TransactionRequest) -> Self {
        Self {
            name: None,
            kind: None,
            tx,
        }
    }

    pub fn with_name(&mut self, name: &str) -> &mut Self {
        self.name = Some(name.to_owned());
        self
    }

    pub fn with_kind(&mut self, kind: &str) -> &mut Self {
        self.kind = Some(kind.to_owned());
        self
    }

    pub fn build(&self) -> NamedTxRequest {
        NamedTxRequest::new(
            self.tx.to_owned(),
            self.name.to_owned(),
            self.kind.to_owned(),
        )
    }
}

impl NamedTxRequest {
    pub fn new(tx: TransactionRequest, name: Option<String>, kind: Option<String>) -> Self {
        Self { name, kind, tx }
    }
}

impl From<TransactionRequest> for NamedTxRequest {
    fn from(tx: TransactionRequest) -> Self {
        Self {
            name: None,
            kind: None,
            tx,
        }
    }
}
