use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::Encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::registry::Registry;

#[derive(Clone, Hash, PartialEq, Eq, Encode)]
struct Labels {
    method: Method,
    path: String,
}

#[derive(Clone, Hash, PartialEq, Eq, Encode)]
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
    let mut buffer = vec![];
    encode(&mut buffer, &registry).unwrap();

    let expected = "# HELP my_counter This is my counter.\n".to_owned()
        + "# TYPE my_counter counter\n"
        + "my_counter_total{method=\"Get\",path=\"/metrics\"} 1\n"
        + "# EOF\n";
    assert_eq!(expected, String::from_utf8(buffer).unwrap());
}

#[cfg(feature = "protobuf")]
mod protobuf {
    use crate::{Labels, Method};
    use prometheus_client::encoding::proto::encode;
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
        let mut metric_set = encode(&registry);
        let mut family: prometheus_client::encoding::proto::MetricFamily =
            metric_set.metric_families.pop().unwrap();
        let metric: prometheus_client::encoding::proto::Metric = family.metrics.pop().unwrap();

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
        let family = Family::<Method, Counter>::default();
        registry.register("my_counter", "This is my counter", family.clone());

        // Record a single HTTP GET request.
        family.get_or_create(&Method::Get).inc();

        // Encode all metrics in the registry in the OpenMetrics protobuf format.
        let mut metric_set = encode(&registry);
        let mut family: prometheus_client::encoding::proto::MetricFamily =
            metric_set.metric_families.pop().unwrap();
        let metric: prometheus_client::encoding::proto::Metric = family.metrics.pop().unwrap();

        let label = &metric.labels[0];
        assert_eq!("Method", label.name);
        assert_eq!("Get", label.value);
    }
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
    }

    let labels = Labels { r#type: 42 };

    let mut buffer = vec![];

    {
        use prometheus_client::encoding::text::Encode;
        labels.encode(&mut buffer).unwrap();
    }

    assert_eq!(
        "type=\"42\"".to_string(),
        String::from_utf8(buffer).unwrap()
    );
}
