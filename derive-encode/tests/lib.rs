use std::sync::Arc;

use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::Registry;

#[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug)]
struct Labels {
    method: Method,
    path: String,
}

#[derive(Clone, Hash, PartialEq, Eq, EncodeLabelValue, Debug)]
enum Method {
    Get,
    #[allow(dead_code)]
    Put,
}

#[test]
fn basic_flow() {
    let mut registry = Registry::default();

    let family = Family::<Labels, Counter>::default();
    registry.register("my_counter", "This is my counter", family.clone());

    // Record a single HTTP GET request.
    family
        .get_or_create(&Labels {
            method: Method::Get,
            path: "/metrics".to_string(),
        })
        .inc();

    // Encode all metrics in the registry in the text format.
    let mut buffer = String::new();
    encode(&mut buffer, &registry).unwrap();

    let expected = "# HELP my_counter This is my counter.\n".to_owned()
        + "# TYPE my_counter counter\n"
        + "my_counter_total{method=\"Get\",path=\"/metrics\"} 1\n"
        + "# EOF\n";
    assert_eq!(expected, buffer);
}

mod protobuf {
    use crate::{Labels, Method};
    use prometheus_client::encoding::protobuf::encode;
    use prometheus_client::encoding::protobuf::openmetrics_data_model;
    use prometheus_client::metrics::counter::Counter;
    use prometheus_client::metrics::family::Family;
    use prometheus_client::registry::Registry;

    #[test]
    fn structs() {
        let mut registry = Registry::default();
        let family = Family::<Labels, Counter>::default();
        registry.register("my_counter", "This is my counter", family.clone());

        // Record a single HTTP GET request.
        family
            .get_or_create(&Labels {
                method: Method::Get,
                path: "/metrics".to_string(),
            })
            .inc();

        // Encode all metrics in the registry in the OpenMetrics protobuf format.
        let mut metric_set = encode(&registry).unwrap();
        let mut family: openmetrics_data_model::MetricFamily =
            metric_set.metric_families.pop().unwrap();
        let metric: openmetrics_data_model::Metric = family.metrics.pop().unwrap();

        let method = &metric.labels[0];
        assert_eq!("method", method.name);
        assert_eq!("Get", method.value);

        let path = &metric.labels[1];
        assert_eq!("path", path.name);
        assert_eq!("/metrics", path.value);
    }

    #[test]
    fn enums() {
        let mut registry = Registry::default();
        let family = Family::<Labels, Counter>::default();
        registry.register("my_counter", "This is my counter", family.clone());

        // Record a single HTTP GET request.
        family
            .get_or_create(&Labels {
                method: Method::Get,
                path: "/metrics".to_string(),
            })
            .inc();

        // Encode all metrics in the registry in the OpenMetrics protobuf format.
        let mut metric_set = encode(&registry).unwrap();
        let mut family: openmetrics_data_model::MetricFamily =
            metric_set.metric_families.pop().unwrap();
        let metric: openmetrics_data_model::Metric = family.metrics.pop().unwrap();

        let label = &metric.labels[0];
        assert_eq!("method", label.name);
        assert_eq!("Get", label.value);
    }
}

#[test]
fn remap_keyword_identifiers() {
    #[derive(EncodeLabelSet, Hash, Clone, Eq, PartialEq, Debug)]
    struct Labels {
        // `r#type` is problematic as `r#` is not a valid OpenMetrics label name
        // but one needs to use keyword identifier syntax (aka. raw identifiers)
        // as `type` is a keyword.
        //
        // Test makes sure `r#type` is replaced by `type` in the OpenMetrics
        // output.
        r#type: u64,
    }

    let mut registry = Registry::default();
    let family = Family::<Labels, Counter>::default();
    registry.register("my_counter", "This is my counter", family.clone());

    // Record a single HTTP GET request.
    family.get_or_create(&Labels { r#type: 42 }).inc();

    // Encode all metrics in the registry in the text format.
    let mut buffer = String::new();
    encode(&mut buffer, &registry).unwrap();

    let expected = "# HELP my_counter This is my counter.\n".to_owned()
        + "# TYPE my_counter counter\n"
        + "my_counter_total{type=\"42\"} 1\n"
        + "# EOF\n";
    assert_eq!(expected, buffer);
}

#[test]
fn arc_string() {
    #[derive(EncodeLabelSet, Hash, Clone, Eq, PartialEq, Debug)]
    struct Labels {
        client_id: Arc<String>,
    }

    let mut registry = Registry::default();
    let family = Family::<Labels, Counter>::default();
    registry.register("my_counter", "This is my counter", family.clone());

    // Record a single HTTP GET request.
    let client_id = Arc::new("client_id".to_string());
    family
        .get_or_create(&Labels {
            client_id: client_id.clone(),
        })
        .inc();

    // Encode all metrics in the registry in the text format.
    let mut buffer = String::new();
    encode(&mut buffer, &registry).unwrap();

    let expected = "# HELP my_counter This is my counter.\n".to_owned()
        + "# TYPE my_counter counter\n"
        + "my_counter_total{client_id=\"client_id\"} 1\n"
        + "# EOF\n";
    assert_eq!(expected, buffer);
}

#[test]
fn flatten() {
    #[derive(EncodeLabelSet, Hash, Clone, Eq, PartialEq, Debug)]
    struct CommonLabels {
        a: u64,
        b: u64,
    }
    #[derive(EncodeLabelSet, Hash, Clone, Eq, PartialEq, Debug)]
    struct Labels {
        unique: u64,
        #[prometheus(flatten)]
        common: CommonLabels,
    }

    let mut registry = Registry::default();
    let family = Family::<Labels, Counter>::default();
    registry.register("my_counter", "This is my counter", family.clone());

    // Record a single HTTP GET request.
    family
        .get_or_create(&Labels {
            unique: 1,
            common: CommonLabels { a: 2, b: 3 },
        })
        .inc();

    // Encode all metrics in the registry in the text format.
    let mut buffer = String::new();
    encode(&mut buffer, &registry).unwrap();

    let expected = "# HELP my_counter This is my counter.\n".to_owned()
        + "# TYPE my_counter counter\n"
        + "my_counter_total{unique=\"1\",a=\"2\",b=\"3\"} 1\n"
        + "# EOF\n";
    assert_eq!(expected, buffer);
}

#[test]
fn skip_encoding_if() {
    fn skip_empty_string(s: &String) -> bool {
        s.is_empty()
    }

    fn skip_zero(n: &u64) -> bool {
        *n == 0
    }

    #[derive(EncodeLabelSet, Hash, Clone, Eq, PartialEq, Debug)]
    struct Labels {
        method: String,
        #[prometheus(skip_encoding_if = "skip_empty_string")]
        path: String,
        #[prometheus(skip_encoding_if = "skip_zero")]
        status_code: u64,
        user_id: u64,
    }

    let mut registry = Registry::default();
    let family = Family::<Labels, Counter>::default();
    registry.register("my_counter", "This is my counter", family.clone());

    family
        .get_or_create(&Labels {
            method: "GET".to_string(),
            path: "".to_string(), // This should be skipped
            status_code: 0,       // This should be skipped
            user_id: 123,
        })
        .inc();

    family
        .get_or_create(&Labels {
            method: "POST".to_string(),
            path: "/api/users".to_string(), // This should not be skipped
            status_code: 200,               // This should not be skipped
            user_id: 456,
        })
        .inc();

    let mut buffer = String::new();
    encode(&mut buffer, &registry).unwrap();

    assert!(buffer.contains("# HELP my_counter This is my counter."));
    assert!(buffer.contains("# TYPE my_counter counter"));
    assert!(buffer.contains("my_counter_total{method=\"GET\",user_id=\"123\"} 1"));
    assert!(buffer.contains("my_counter_total{method=\"POST\",path=\"/api/users\",status_code=\"200\",user_id=\"456\"} 1"));
    assert!(buffer.contains("# EOF"));

    assert!(!buffer.contains("path=\"\""));
    assert!(!buffer.contains("status_code=\"0\""));
}

#[test]
fn build() {
    let t = trybuild::TestCases::new();
    t.pass("tests/build/redefine-prelude-symbols.rs")
}
