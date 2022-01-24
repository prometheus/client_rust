use prometheus_client::encoding::text::{encode, EncodeMetric, Encoder};
use prometheus_client::metrics::MetricType;
use prometheus_client::registry::Registry;

/// Showcasing encoding of custom metrics.
///
/// Related to the concept of "Custom Collectors" in other implementations.
///
/// [`MyCustomMetric`] generates and encodes a random number on each scrape.
struct MyCustomMetric {}

impl EncodeMetric for MyCustomMetric {
    fn encode(&self, mut encoder: Encoder) -> Result<(), std::io::Error> {
        // This method is called on each Prometheus server scrape. Allowing you
        // to execute whatever logic is needed to generate and encode your
        // custom metric.
        //
        // While the `Encoder`'s builder pattern should guide you well and makes
        // many mistakes impossible at the type level, do keep in mind that
        // "with great power comes great responsibility". E.g. every CPU cycle
        // spend in this method delays the response send to the Prometheus
        // server.

        encoder
            .no_suffix()?
            .no_bucket()?
            .encode_value(rand::random::<u32>())?
            .no_exemplar()?;

        Ok(())
    }

    fn metric_type(&self) -> prometheus_client::metrics::MetricType {
        MetricType::Unknown
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

    let mut encoded = Vec::new();
    encode(&mut encoded, &registry).unwrap();

    println!("Scrape output:\n{:?}", String::from_utf8(encoded).unwrap());
}
