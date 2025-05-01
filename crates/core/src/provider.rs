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
use tracing::debug;

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
                                debug!("tx delivered. hash: {}, id: {id}", payload.get());
                            }
                        }
                        ResponsePacket::Batch(_) => {}
                    }
                }
            } else if let Err(err) = &res {
                debug!("RPC Error: {err}");
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

#[cfg(test)]
pub mod tests {
    use alloy::rpc::json_rpc::Id;
    use tracing::Level;
    use tracing_subscriber::FmtSubscriber;

    use super::*;

    static PROM: OnceCell<prometheus::Registry> = OnceCell::const_new();
    static HIST: OnceCell<prometheus::HistogramVec> = OnceCell::const_new();
    static TRACING_INIT: std::sync::Once = std::sync::Once::new();

    #[derive(Clone)]
    struct FailingService;

    impl Service<RequestPacket> for FailingService {
        type Response = ResponsePacket;
        type Error = TransportError;
        type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

        fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn call(&mut self, _req: RequestPacket) -> Self::Future {
            let err = TransportError::Transport(alloy::transports::TransportErrorKind::Custom(
                "bummer".into(),
            ));
            Box::pin(async move { Err(err) })
        }
    }

    #[tokio::test]
    async fn bad_request_logs_error() -> Result<()> {
        TRACING_INIT.call_once(|| {
            let subscriber = FmtSubscriber::builder()
                .with_max_level(Level::DEBUG)
                .finish();
            tracing::subscriber::set_global_default(subscriber)
                .expect("setting default subscriber failed");
        });

        let layer = LoggingLayer::new(&PROM, &HIST).await;
        let mut service = layer.layer(FailingService);
        let req = RequestPacket::Single(
            alloy::rpc::json_rpc::Request::<Vec<String>>::new(
                "eth_sendRawTransaction",
                Id::Number(1),
                vec![],
            )
            .serialize()
            .unwrap(),
        );
        let res = service.call(req).await;
        assert!(res.is_err());
        if let Err(TransportError::Transport(err)) = res {
            assert_eq!(err.to_string(), "bummer");
        } else {
            panic!("Expected a transport error");
        }

        Ok(())
    }
}
