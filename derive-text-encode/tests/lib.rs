use prometheus_client::encoding::text::{encode, Encode};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::Registry;

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

#[test]
fn remap_keyword_identifiers() {
    #[derive(Encode, Hash, Clone, Eq, PartialEq)]
    struct Labels {
        // `r#type` is problematic as `r#` is not a valid OpenMetrics label name
        // but one needs to use keyword identifier syntax (aka. raw identifiers)
        // as `type` is a keyword.
        //
        // Test makes sure `r#type` is replaced by `type` in the OpenMetrics
        // output.
        r#type: u64,
    };

    let labels = Labels { r#type: 42 };

    let mut buffer = vec![];

    labels.encode(&mut buffer);

    assert_eq!(
        "type=\"42\"".to_string(),
        String::from_utf8(buffer).unwrap()
    );
}
