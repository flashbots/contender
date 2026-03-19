use jsonrpsee::proc_macros::rpc;

#[rpc(server)]
pub trait ContenderRpc {
    #[method(name = "status")]
    async fn status(&self) -> jsonrpsee::core::RpcResult<String>;
}

pub struct ContenderServer;

#[async_trait::async_trait]
impl ContenderRpcServer for ContenderServer {
    async fn status(&self) -> jsonrpsee::core::RpcResult<String> {
        Ok("system has become self-aware".to_string())
    }
}
