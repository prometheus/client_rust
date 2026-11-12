use prometheus_client::encoding::negotiate::{negotiate, Format};
use prometheus_client::encoding::EncodeLabelValue;
use prometheus_client::encoding::{prometheus_text, text, EncodeLabelSet};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::Registry;

use std::sync::Arc;

use tide::{Middleware, Next, Request, Result};

#[async_std::main]
async fn main() -> std::result::Result<(), std::io::Error> {
    tide::log::start();

    let mut registry = Registry::default();
    let http_requests_total = Family::<Labels, Counter>::default();
    registry.register(
        "http_requests",
        "Number of HTTP requests",
        http_requests_total.clone(),
    );

    let middleware = MetricsMiddleware {
        http_requests_total,
    };
    let mut app = tide::with_state(State {
        registry: Arc::new(registry),
    });

    app.with(middleware);
    app.at("/").get(|_| async { Ok("Hello, world!") });
    app.at("/metrics")
        .get(|req: tide::Request<State>| async move {
            let accept = req.header("Accept").map(|values| values.as_str());
            let media_type = negotiate(
                accept,
                [Format::PrometheusText0_0_4, Format::OpenMetricsText1_0_0],
            );

            let mut encoded = String::new();
            match media_type.format() {
                Format::PrometheusText0_0_4 => {
                    prometheus_text::encode(&mut encoded, &req.state().registry).unwrap()
                }
                Format::OpenMetricsText1_0_0 => {
                    text::encode(&mut encoded, &req.state().registry).unwrap()
                }
                // `Format` is non-exhaustive, and only offered formats are returned.
                _ => unreachable!(),
            }
            let response = tide::Response::builder(200)
                .body(encoded)
                .content_type(media_type.content_type())
                .build();
            Ok(response)
        });
    app.listen("127.0.0.1:8080").await?;

    Ok(())
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct Labels {
    method: Method,
    path: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
enum Method {
    Get,
    Put,
}

#[derive(Clone)]
struct State {
    registry: Arc<Registry>,
}

#[derive(Default)]
struct MetricsMiddleware {
    http_requests_total: Family<Labels, Counter>,
}

#[tide::utils::async_trait]
impl Middleware<State> for MetricsMiddleware {
    async fn handle(&self, req: Request<State>, next: Next<'_, State>) -> Result {
        let method = match req.method() {
            http_types::Method::Get => Method::Get,
            http_types::Method::Put => Method::Put,
            _ => todo!(),
        };
        let path = req.url().path().to_string();
        let _count = self
            .http_requests_total
            .get_or_create(&Labels { method, path })
            .inc();

        let res = next.run(req).await;
        Ok(res)
    }
}
