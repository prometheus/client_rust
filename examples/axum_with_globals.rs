use axum::{
    body::Body,
    http::{header::CONTENT_TYPE, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use prometheus_client::{
    encoding::text::encode,
    metrics::{counter::Counter, family::Family},
    registry::{Metric, Registry},
};
use prometheus_client_derive_encode::{EncodeLabelSet, EncodeLabelValue};
use std::{
    sync::{LazyLock, Mutex},
    time::Duration,
};
use tokio::time;

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

pub fn register_metric_to_global_registry<MetricType: Metric + Clone + Default>(
    name: &str,
    help: &str,
) -> MetricType {
    let metric: MetricType = MetricType::default();
    let mut registry = REGISTRY
        .lock()
        .unwrap_or_else(|_| panic!("Cannot lock metrics registry to create {name} metric"));
    registry.register(name, help, metric.clone());
    metric
}

pub async fn metrics_handler() -> impl IntoResponse {
    let mut buffer = String::new();
    {
        let registry = REGISTRY
            .lock()
            .expect("could not acquire a lock on registry to push metrics");
        encode(&mut buffer, &registry).unwrap();
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(
            CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8",
        )
        .body(Body::from(buffer))
        .unwrap()
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum Type {
    Request,
    BackgroundTask,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct MethodLabels {
    pub method: Type,
}

static METRIC: LazyLock<Family<MethodLabels, Counter>> = LazyLock::new(|| {
    register_metric_to_global_registry("requests_and_tasks", "Count of requests & tasks")
});

pub async fn some_handler() -> impl IntoResponse {
    METRIC
        .get_or_create(&MethodLabels {
            method: Type::Request,
        })
        .inc();
    "okay".to_string()
}

#[tokio::main]
async fn main() {
    let router = Router::new()
        .route("/metrics", get(metrics_handler))
        .route("/handler", get(some_handler));
    let port = 8080;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();

    let (listener, _background_task) =
        tokio::join!(axum::serve(listener, router), background_task());
    listener.unwrap()
}

async fn background_task() {
    let mut interval = time::interval(Duration::from_secs(2));
    loop {
        interval.tick().await;
        let current = METRIC
            .get_or_create(&MethodLabels {
                method: Type::BackgroundTask,
            })
            .inc();
        println!("executed task: {current} times");
    }
}
