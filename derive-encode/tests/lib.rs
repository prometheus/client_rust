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
