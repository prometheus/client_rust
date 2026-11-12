use actix_web::http::header::ACCEPT;
use actix_web::middleware::Compress;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder, Result};
use prometheus_client::encoding::negotiate::{negotiate, Format};
use prometheus_client::encoding::{prometheus_text, text};
use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::Registry;

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
pub struct MethodLabels {
    pub method: Method,
}

pub struct Metrics {
    requests: Family<MethodLabels, Counter>,
}

impl Metrics {
    pub fn inc_requests(&self, method: Method) {
        self.requests.get_or_create(&MethodLabels { method }).inc();
    }
}

pub struct AppState {
    pub registry: Registry,
}

pub async fn metrics_handler(req: HttpRequest, state: web::Data<AppState>) -> Result<HttpResponse> {
    let accept = req
        .headers()
        .get(ACCEPT)
        .and_then(|value| value.to_str().ok());
    let media_type = negotiate(
        accept,
        [Format::PrometheusText0_0_4, Format::OpenMetricsText1_0_0],
    );

    let mut body = String::new();
    match media_type.format() {
        Format::PrometheusText0_0_4 => prometheus_text::encode(&mut body, &state.registry).unwrap(),
        Format::OpenMetricsText1_0_0 => text::encode(&mut body, &state.registry).unwrap(),
        // `Format` is non-exhaustive, and only offered formats are returned.
        _ => unreachable!(),
    }

    Ok(HttpResponse::Ok()
        .content_type(media_type.content_type())
        .body(body))
}

pub async fn some_handler(metrics: web::Data<Metrics>) -> impl Responder {
    metrics.inc_requests(Method::Get);
    "okay".to_string()
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let metrics = web::Data::new(Metrics {
        requests: Family::default(),
    });
    let mut state = AppState {
        registry: Registry::default(),
    };
    state
        .registry
        .register("requests", "Count of requests", metrics.requests.clone());
    let state = web::Data::new(state);

    HttpServer::new(move || {
        App::new()
            .wrap(Compress::default())
            .app_data(metrics.clone())
            .app_data(state.clone())
            .service(web::resource("/metrics").route(web::get().to(metrics_handler)))
            .service(web::resource("/handler").route(web::get().to(some_handler)))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
