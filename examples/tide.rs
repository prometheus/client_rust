use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::encoding::EncodeLabelValue;
use prometheus_client::http::ScrapeHandler;
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
        scrape_handler: ScrapeHandler::default(),
    });

    app.with(middleware);
    app.at("/").get(|_| async { Ok("Hello, world!") });
    app.at("/metrics")
        .get(|req: tide::Request<State>| async move {
            let state = req.state();
            let result = state.scrape_handler.handle(&state.registry, |name| {
                req.header(name).map(|values| values.as_str())
            });

            match result {
                Ok(response) => {
                    let mut builder = tide::Response::builder(response.status());
                    for (name, value) in response.headers() {
                        builder = builder.header(name, value);
                    }
                    Ok(builder.body(response.into_body()).build())
                }
                Err(err) => {
                    eprintln!("{err}");
                    Ok(tide::Response::builder(err.status())
                        .body(err.to_string())
                        .build())
                }
            }
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
    scrape_handler: ScrapeHandler,
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
