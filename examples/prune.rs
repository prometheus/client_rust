use prometheus_client::encoding::{text::encode, EncodeMetric, MetricEncoder};
use prometheus_client::metrics::counter::Atomic;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::{MetricType, TypedMetric};
use prometheus_client::registry::Registry;
use prometheus_client_derive_encode::EncodeLabelSet;
use std::fmt::Error;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tokio::time::Instant;

// The 'prune' example shows an advanced use case of a custom metric type that records it's last record date.
// Then, label sets older than a certain time period are removed from the label set.
fn main() {
    let mut registry = Registry::default();

    let metric: Family<Labels, MyCounter> = Family::default();
    registry.register("my_custom_metric", "test", metric.clone());
    // First we record two label sets, apple and banana.
    metric
        .get_or_create(&Labels {
            name: "apple".to_string(),
        })
        .inc();
    metric
        .get_or_create(&Labels {
            name: "banana".to_string(),
        })
        .inc();

    let mut encoded = String::new();
    encode(&mut encoded, &registry).unwrap();

    println!("Scrape output:\n{}", encoded);
    thread::sleep(Duration::from_secs(1));

    // We update only 'banana'
    metric
        .get_or_create(&Labels {
            name: "banana".to_string(),
        })
        .inc();
    let now = Instant::now();
    // Retain only metrics set within the last second.
    metric.retain(|_labels, counter| {
        let last = counter.last_access.lock().unwrap().unwrap();
        now.saturating_duration_since(last) < Duration::from_secs(1)
    });

    // 'apple' should be removed now
    let mut encoded = String::new();
    encode(&mut encoded, &registry).unwrap();

    println!("Scrape output:\n{}", encoded);
}

#[derive(Default, Debug)]
struct MyCounter {
    value: Arc<AtomicU64>,
    last_access: Arc<Mutex<Option<Instant>>>,
}

impl TypedMetric for MyCounter {
    const TYPE: MetricType = MetricType::Counter;
}

impl MyCounter {
    pub fn get(&self) -> u64 {
        self.value.get()
    }
    pub fn inc(&self) -> u64 {
        let mut last = self.last_access.lock().unwrap();
        *last = Some(Instant::now());
        self.value.inc()
    }
}

impl EncodeMetric for MyCounter {
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), Error> {
        encoder.encode_counter::<prometheus_client::encoding::NoLabelSet, _, u64>(&self.get(), None)
    }

    fn metric_type(&self) -> MetricType {
        todo!()
    }
}

#[derive(Clone, Hash, Default, Debug, PartialEq, Eq, EncodeLabelSet)]
struct Labels {
    name: String,
}
