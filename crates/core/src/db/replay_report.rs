use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReplayReportRequest {
    pub rpc_url_id: u64,
    pub gas_per_second: u64,
    pub gas_used: u64,
}

impl ReplayReportRequest {
    pub fn new() -> Self {
        Self {
            rpc_url_id: 0,
            gas_per_second: 0,
            gas_used: 0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReplayReport {
    pub id: u64,
    #[serde(flatten)]
    req: ReplayReportRequest,
}

impl ReplayReport {
    pub fn new(id: u64, req: ReplayReportRequest) -> Self {
        Self { id, req }
    }

    pub fn gas_used(&self) -> u64 {
        self.req.gas_used
    }

    pub fn gas_per_second(&self) -> u64 {
        self.req.gas_per_second
    }

    pub fn rpc_url_id(&self) -> u64 {
        self.req.rpc_url_id
    }
}
