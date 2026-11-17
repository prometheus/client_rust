use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use prometheus_client::http::ScrapeHandler;
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
    pub scrape_handler: ScrapeHandler,
}

pub async fn metrics_handler(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let result = state.scrape_handler.handle(&state.registry, |name| {
        headers.get(name).and_then(|value| value.to_str().ok())
    });

    match result {
        Ok(response) => {
            let mut builder = Response::builder().status(response.status());
            for (name, value) in response.headers() {
                builder = builder.header(name, value);
            }
            builder.body(Body::from(response.into_body())).unwrap()
        }
        Err(err) => {
            eprintln!("{err}");
            let status = StatusCode::from_u16(err.status()).unwrap();
            (status, err.to_string()).into_response()
        }
    }
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
        scrape_handler: ScrapeHandler::default(),
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
