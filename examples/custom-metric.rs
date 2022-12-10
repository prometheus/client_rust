use prometheus_client::encoding::{text::encode, EncodeMetric, MetricEncoder};
use prometheus_client::metrics::MetricType;
use prometheus_client::registry::Registry;

/// Showcasing encoding of custom metrics.
///
/// Related to the concept of "Custom Collectors" in other implementations.
///
/// [`MyCustomMetric`] generates and encodes a random number on each scrape.
#[derive(Debug)]
struct MyCustomMetric {}

impl EncodeMetric for MyCustomMetric {
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        // This method is called on each Prometheus server scrape. Allowing you
        // to execute whatever logic is needed to generate and encode your
        // custom metric.
        //
        // Do keep in mind that "with great power comes great responsibility".
        // E.g. every CPU cycle spend in this method delays the response send to
        // the Prometheus server.

        encoder.encode_counter::<(), _, u64>(&rand::random::<u64>(), None)
    }

    fn metric_type(&self) -> prometheus_client::metrics::MetricType {
        MetricType::Counter
    }
}

fn main() {
    let mut registry = Registry::default();

    let metric = MyCustomMetric {};
    registry.register(
        "my_custom_metric",
        "Custom metric returning a random number on each scrape",
        metric,
    );

    let mut encoded = String::new();
    encode(&mut encoded, &registry).unwrap();

    println!("Scrape output:\n{:?}", encoded);
}
