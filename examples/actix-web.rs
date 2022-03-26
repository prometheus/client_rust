use std::sync::Mutex;

use actix_web::{web, App, HttpResponse, HttpServer, Responder, Result};
use prometheus_client::encoding::text::{encode, Encode};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::Registry;

#[derive(Clone, Hash, PartialEq, Eq, Encode)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone, Hash, PartialEq, Eq, Encode)]
pub struct MethodLabels {
    pub method: Method,
}

/// Holds all metrics.
/// We shouldn't store metrics inside a MetricsCollector -
/// otherwise we would have to take a lock for each increment of any metric
pub struct Metrics {
    requests: Family<MethodLabels, Counter>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            requests: Family::default(),
        }
    }

    pub fn inc_requests(&self, method: Method) {
        self.requests.get_or_create(&MethodLabels { method }).inc();
    }
}

/// Registers and collects metrics
pub struct MetricsCollector {
    registry: Registry,
}

impl MetricsCollector {
    pub fn new(metrics: &Metrics) -> Self {
        let mut collector = Self {
            registry: Registry::default(),
        };
        collector.register_metrics(metrics);
        collector
    }

    fn register_metrics(&mut self, metrics: &Metrics) {
        self.registry.register(
            "requests",
            "Count of requests",
            Box::new(metrics.requests.clone()),
        );
    }

    pub fn collect(&self) -> Result<String, std::io::Error> {
        let mut buf = Vec::new();
        encode(&mut buf, &self.registry)?;

        let buf_str = std::str::from_utf8(buf.as_slice()).unwrap().to_string();
        Ok(buf_str)
    }
}

pub async fn metrics_handler(
    metrics_collector: web::Data<Mutex<MetricsCollector>>,
) -> Result<HttpResponse> {
    // TODO: find the way without locking the mutex
    let body: String = metrics_collector.lock().unwrap().collect()?;
    Ok(HttpResponse::Ok().body(body))
}

pub async fn some_handler(metrics: web::Data<Metrics>) -> impl Responder {
    metrics.inc_requests(Method::Get);
    format!("okay")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let metrics = web::Data::new(Metrics::new());
    // We have to wrap it with Mutex because `Registry` doesn't implement Clone-trait
    let metrics_collector = web::Data::new(Mutex::new(MetricsCollector::new(&metrics)));

    HttpServer::new(move || {
        App::new()
            .app_data(metrics.clone())
            .app_data(metrics_collector.clone())
            .service(web::resource("/metrics").route(web::get().to(metrics_handler)))
            .service(web::resource("/handler").route(web::get().to(some_handler)))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
