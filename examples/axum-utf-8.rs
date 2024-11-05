use axum::body::Body;
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::{Registry, RegistryBuilder};
use std::sync::Arc;
use tokio::sync::Mutex;
use prometheus_client::encoding::EscapingScheme::UnderscoreEscaping;
use prometheus_client::encoding::negotiate_escaping_scheme;
use prometheus_client::encoding::ValidationScheme::UTF8Validation;

#[derive(Debug)]
pub struct Metrics {
    requests: Family<Vec<(String, String)>, Counter>,
}

impl Metrics {
    pub fn inc_requests(&self, method: String) {
        self.requests.get_or_create(&vec![("method.label".to_owned(), method)]).inc();
    }
}

#[derive(Debug)]
pub struct AppState {
    pub registry: Registry,
}

pub async fn metrics_handler(State(state): State<Arc<Mutex<AppState>>>, headers: HeaderMap) -> impl IntoResponse {
    let mut state = state.lock().await;
    let mut buffer = String::new();
    if let Some(accept) = headers.get("Accept") {
        let escaping_scheme = negotiate_escaping_scheme(
            accept.to_str().unwrap(),
            state.registry.escaping_scheme()
        );
        state.registry.set_escaping_scheme(escaping_scheme);
    }
    encode(&mut buffer, &state.registry).unwrap();

    Response::builder()
        .status(StatusCode::OK)
        .header(
            CONTENT_TYPE,
            "application/openmetrics-text; version=1.0.0; charset=utf-8; escaping=".to_owned() + state.registry.escaping_scheme().as_str(),
        )
        .body(Body::from(buffer))
        .unwrap()
}

pub async fn some_handler(State(metrics): State<Arc<Mutex<Metrics>>>) -> impl IntoResponse {
    metrics.lock().await.inc_requests("Get".to_owned());
    "okay".to_string()
}

#[tokio::main]
async fn main() {
    let metrics = Metrics {
        requests: Family::default(),
    };
    let mut state = AppState {
        registry: RegistryBuilder::new()
            .with_name_validation_scheme(UTF8Validation)
            .with_escaping_scheme(UnderscoreEscaping)
            .build(),
    };
    state
        .registry
        .register("requests.count", "Count of requests", metrics.requests.clone());
    let metrics = Arc::new(Mutex::new(metrics));
    let state = Arc::new(Mutex::new(state));

    let router = Router::new()
        .route("/metrics", get(metrics_handler))
        .with_state(state)
        .route("/handler", get(some_handler))
        .with_state(metrics);
    let port = 8080;
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();

    axum::serve(listener, router).await.unwrap();
}
