use open_metrics_client::encoding::text::{encode, Encode};
use open_metrics_client::metrics::counter::Counter;
use open_metrics_client::metrics::family::Family;
use open_metrics_client::registry::Registry;

#[test]
fn basic_flow() {
    let mut registry = Registry::default();
    #[derive(Clone, Hash, PartialEq, Eq, Encode)]
    struct Labels {
        method: Method,
        path: String,
    };

    #[derive(Clone, Hash, PartialEq, Eq, Encode)]
    enum Method {
        GET,
        #[allow(dead_code)]
        PUT,
    };

    let family = Family::<Labels, Counter>::default();
    registry.register("my_counter", "This is my counter", family.clone());

    // Record a single HTTP GET request.
    family
        .get_or_create(&Labels {
            method: Method::GET,
            path: "/metrics".to_string(),
        })
        .inc();

    // Encode all metrics in the registry in the text format.
    let mut buffer = vec![];
    encode(&mut buffer, &registry).unwrap();

    let expected = "# HELP my_counter This is my counter.\n".to_owned()
        + "# TYPE my_counter counter\n"
        + "my_counter_total{method=\"GET\",path=\"/metrics\"} 1\n"
        + "# EOF\n";
    assert_eq!(expected, String::from_utf8(buffer).unwrap());
}
