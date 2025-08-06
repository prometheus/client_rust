use prometheus_client::{
    encoding::text::encode,
    metrics::counter::Counter,
    metrics::gauge::Gauge,
    registry::{Registrant as _, Registry},
};
use prometheus_client_derive::Registrant;

#[test]
fn build() {
    let t = trybuild::TestCases::new();
    t.pass("tests/build/01-parse.rs");
    t.pass("tests/build/02-redefine-prelude-symbols.rs");
    t.compile_fail("tests/build/03-rename.rs");
    t.compile_fail("tests/build/04-unit.rs");
    t.pass("tests/build/05-help.rs");
    t.pass("tests/build/06-attributes.rs");
}

#[test]
fn sanity() {
    #[derive(Registrant)]
    struct HttpServer {
        /// Number of HTTP requests received
        /// from the client
        #[registrant(rename = "http_requests")]
        requests: Counter,

        /// Memory usage in bytes
        /// of the server
        #[registrant(unit = "bytes")]
        memory_usage: Gauge,

        #[registrant(skip)]
        #[allow(dead_code)]
        skip: (),
    }

    let mut registry = Registry::default();
    let http_server = HttpServer {
        requests: Counter::default(),
        memory_usage: Gauge::default(),
        skip: (),
    };
    http_server.register(&mut registry);

    let mut buf = String::new();
    encode(&mut buf, &registry).unwrap();

    let expected = [
        "# HELP http_requests Number of HTTP requests received from the client.",
        "# TYPE http_requests counter",
        "http_requests_total 0",
        "# HELP memory_usage_bytes Memory usage in bytes of the server.",
        "# TYPE memory_usage_bytes gauge",
        "# UNIT memory_usage_bytes bytes",
        "memory_usage_bytes 0",
        "# EOF\n",
    ]
    .join("\n");
    assert_eq!(buf, expected);
}
