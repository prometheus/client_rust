use axum::body::Body;
use axum::extract::State;
use axum::http::header::{ACCEPT, CONTENT_TYPE, VARY};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use prometheus_client::encoding::negotiate::{negotiate, Format};
use prometheus_client::encoding::{prometheus_text, text};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::Registry;
use prometheus_client_derive_encode::{EncodeLabelSet, EncodeLabelValue};
use std::sync::Arc;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct MethodLabels {
    pub method: Method,
}

#[derive(Debug)]
pub struct Metrics {
    requests: Family<MethodLabels, Counter>,
}

impl Metrics {
    pub fn inc_requests(&self, method: Method) {
        self.requests.get_or_create(&MethodLabels { method }).inc();
    }
}

#[derive(Debug)]
pub struct AppState {
    pub registry: Registry,
}

pub async fn metrics_handler(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let accept = headers.get(ACCEPT).and_then(|value| value.to_str().ok());
    let media_type = negotiate(
        accept,
        [Format::PrometheusText0_0_4, Format::OpenMetricsText1_0_0],
    );

    let mut buffer = String::new();
    match media_type.format() {
        Format::PrometheusText0_0_4 => {
            prometheus_text::encode(&mut buffer, &state.registry).unwrap()
        }
        Format::OpenMetricsText1_0_0 => text::encode(&mut buffer, &state.registry).unwrap(),
        // `Format` is non-exhaustive, and only offered formats are returned.
        _ => unreachable!(),
    }

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, media_type.content_type())
        .header(VARY, "Accept")
        .body(Body::from(buffer))
        .unwrap()
}

pub async fn some_handler(State(metrics): State<Arc<Metrics>>) -> impl IntoResponse {
    metrics.inc_requests(Method::Get);
    "okay".to_string()
}

#[tokio::main]
async fn main() {
    let metrics = Metrics {
        requests: Family::default(),
    };
    let mut state = AppState {
        registry: Registry::default(),
    };
    state
        .registry
        .register("requests", "Count of requests", metrics.requests.clone());
    let metrics = Arc::new(metrics);
    let state = Arc::new(state);

    let router = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(state)
        .route("/handler", get(some_handler))
        .with_state(metrics);
    let port = 8080;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .unwrap();

    axum::serve(listener, router).await.unwrap();
}
