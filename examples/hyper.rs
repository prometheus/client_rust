use futures::future::BoxFuture;
use http_body_util::{combinators, BodyExt, Full};
use hyper::{
    body::{Bytes, Incoming},
    server::conn::http1,
    service::service_fn,
    Request, Response,
};
use hyper_util::rt::TokioIo;
use prometheus_client::{http::ScrapeHandler, metrics::counter::Counter, registry::Registry};
use std::{
    error::Error,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{
    net::TcpListener,
    pin,
    signal::unix::{signal, SignalKind},
};

#[tokio::main]
async fn main() {
    let request_counter: Counter<u64> = Default::default();

    let mut registry = <Registry>::with_prefix("tokio_hyper_example");

    registry.register(
        "requests",
        "How many requests the application has received",
        request_counter.clone(),
    );

    // Spawn a server to serve the OpenMetrics endpoint.
    let metrics_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8001);
    start_metrics_server(metrics_addr, registry).await
}

/// Start a HTTP server to report metrics.
pub async fn start_metrics_server(metrics_addr: SocketAddr, registry: Registry) {
    eprintln!("Starting metrics server on {metrics_addr}");

    let registry = Arc::new(registry);
    let scrape_handler = Arc::new(ScrapeHandler::default());

    let tcp_listener = TcpListener::bind(metrics_addr).await.unwrap();
    let server = http1::Builder::new();
    while let Ok((stream, _)) = tcp_listener.accept().await {
        let mut shutdown_stream = signal(SignalKind::terminate()).unwrap();
        let io = TokioIo::new(stream);
        let server_clone = server.clone();
        let registry_clone = registry.clone();
        let scrape_handler_clone = scrape_handler.clone();
        tokio::task::spawn(async move {
            let conn = server_clone.serve_connection(
                io,
                service_fn(make_handler(registry_clone, scrape_handler_clone)),
            );
            pin!(conn);
            tokio::select! {
                _ = conn.as_mut() => {}
                _ = shutdown_stream.recv() => {
                    conn.as_mut().graceful_shutdown();
                }
            }
        });
    }
}

/// Boxed HTTP body for responses
type BoxBody = combinators::BoxBody<Bytes, hyper::Error>;

/// This function returns a HTTP handler (i.e. another function)
pub fn make_handler(
    registry: Arc<Registry>,
    scrape_handler: Arc<ScrapeHandler>,
) -> impl Fn(Request<Incoming>) -> BoxFuture<'static, io::Result<Response<BoxBody>>> {
    // This closure accepts a request and responds with the encoding of our metrics that the
    // request accepts.
    move |req: Request<Incoming>| {
        let result = scrape_handler.handle(&registry, |name| {
            req.headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
        });

        let response = match result {
            Ok(response) => {
                let mut builder = Response::builder().status(response.status());
                for (name, value) in response.headers() {
                    builder = builder.header(name, value);
                }
                builder.body(full(Bytes::from(response.into_body())))
            }
            Err(err) => {
                eprint!("{err}");
                let mut source = err.source();
                while let Some(cause) = source {
                    eprint!(": {cause}");
                    source = cause.source();
                }
                eprintln!();
                Response::builder()
                    .status(err.status())
                    .body(full(Bytes::from(err.to_string())))
            }
        };

        Box::pin(async move { Ok(response.unwrap()) })
    }
}

/// helper function to build a full boxed body
pub fn full(body: Bytes) -> BoxBody {
    Full::new(body).map_err(|never| match never {}).boxed()
}
