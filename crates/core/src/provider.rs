use std::{
    fmt::Debug,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use alloy::{
    rpc::json_rpc::{RequestPacket, ResponsePacket},
    transports::TransportError,
};
use eyre::Result;
use prometheus::{HistogramOpts, HistogramVec, Registry};
use tokio::sync::OnceCell;
use tower::{Layer, Service};

pub const RPC_REQUEST_LATENCY_ID: &str = "rpc_request_latency_seconds";

/// A layer to be used with `ClientBuilder::layer` that logs request id with tx hash when calling eth_sendRawTransaction.
pub struct LoggingLayer {
    latency_histogram: &'static OnceCell<HistogramVec>,
}

impl LoggingLayer {
    /// Creates a new `LoggingLayer` and initialize metrics.
    pub async fn new(
        registry: &OnceCell<Registry>,
        latency_histogram: &'static OnceCell<HistogramVec>,
    ) -> Self {
        init_metrics(registry, latency_histogram).await;
        Self { latency_histogram }
    }
}

// Implement tower::Layer for LoggingLayer.
impl<S> Layer<S> for LoggingLayer {
    type Service = LoggingService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LoggingService {
            inner,
            latency_histogram: self.latency_histogram,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoggingService<S> {
    inner: S,
    latency_histogram: &'static OnceCell<HistogramVec>,
}

impl<S> Service<RequestPacket> for LoggingService<S>
where
    // Constraints on the service.
    S: Service<RequestPacket, Response = ResponsePacket, Error = TransportError>,
    S::Future: Send + 'static,
    S::Response: Send + 'static + Debug,
    S::Error: Send + 'static + Debug,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: RequestPacket) -> Self::Future {
        let mut id = 0;
        let mut method: Option<String> = None;
        match &req {
            RequestPacket::Single(inner_req) => {
                method = Some(inner_req.method().to_string());
                if inner_req.method() == "eth_sendRawTransaction" {
                    id = inner_req.id().as_number().unwrap_or_default();
                }
            }
            RequestPacket::Batch(_) => {}
        }

        let start_time = tokio::time::Instant::now();
        let fut = self.inner.call(req);
        let latency_histogram = self.latency_histogram.get();

        Box::pin(async move {
            let res = fut.await;
            if let Ok(res) = &res {
                let elapsed = start_time.elapsed().as_secs_f64();
                if let Some(h) = latency_histogram {
                    if let Some(method) = method {
                        h.with_label_values(&[method.as_str()]).observe(elapsed);
                    }
                }
                if id != 0 {
                    match res {
                        ResponsePacket::Single(inner_res) => {
                            if let Some(payload) = inner_res.payload.as_success() {
                                println!("tx delivered. hash: {}, id: {id}", payload.get());
                            }
                        }
                        ResponsePacket::Batch(_) => {}
                    }
                }
            }
            res
        })
    }
}

async fn init_metrics(registry: &OnceCell<Registry>, latency_hist: &OnceCell<HistogramVec>) {
    let reg = Registry::new();

    let histogram_vec = HistogramVec::new(
        HistogramOpts::new(RPC_REQUEST_LATENCY_ID, "Latency of requests in seconds")
            .buckets(vec![0.0001, 0.001, 0.01, 0.05, 0.1, 0.25, 0.5]),
        &["rpc_method"],
    )
    .expect("histogram_vec");
    reg.register(Box::new(histogram_vec.clone()))
        .expect("histogram registered");

    registry.set(reg).unwrap_or(());
    latency_hist.set(histogram_vec).unwrap_or(());
}
