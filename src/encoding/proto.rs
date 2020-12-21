use crate::counter::{Atomic, Counter};


use crate::registry::Registry;
use prost::bytes::BufMut;
use prost::Message;


// TODO: Open issue on open metrics repo asking to consider using `optional` fields in proto spec.
//
// https://github.com/protocolbuffers/protobuf/blob/v3.12.0/docs/field_presence.md
// https://stackoverflow.com/questions/42622015/how-to-define-an-optional-field-in-protobuf-3

mod open_metrics_proto {
    include!(concat!(env!("OUT_DIR"), "/openmetrics.rs"));
}

fn encode<B: BufMut, M: ToMetrics>(mut buf: &mut B, registry: &Registry<M>) {
    let families = registry
        .iter()
        .map(|(desc, metric)| {
            open_metrics_proto::MetricFamily {
                name: desc.name().to_string(),
                // TODO: Fix.
                r#type: 0,
                unit: "".to_string(),
                help: desc.help().to_string(),
                metrics: metric.to_metrics(),
            }
        })
        .collect();

    let set = open_metrics_proto::MetricSet {
        metric_families: families,
    };

    set.encode(&mut buf).unwrap();
}

trait ToMetrics {
    fn to_metrics(&self) -> Vec<open_metrics_proto::Metric>;
}

impl<A> ToMetrics for Counter<A>
where
    A: Atomic,
    <A as Atomic>::Number: Into<f64>,
{
    fn to_metrics(&self) -> Vec<open_metrics_proto::Metric> {
        let value = open_metrics_proto::metric_point::Value::CounterValue(
            open_metrics_proto::CounterValue {
                created: None,
                exemplar: None,
                // TODO: Use double or int.
                total: Some(open_metrics_proto::counter_value::Total::DoubleValue(
                    self.get().into(),
                )),
            },
        );
        let metric = open_metrics_proto::Metric {
            labels: vec![],
            metric_points: vec![open_metrics_proto::MetricPoint {
                timestamp: None,
                value: Some(value),
            }],
        };
        vec![metric]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Descriptor;
    use std::sync::atomic::AtomicU32;

    #[test]
    fn encode_counter_family() {
        let mut registry = Registry::new();
        let counter = Counter::<AtomicU32>::new();
        registry.register(
            Descriptor::new("counter", "My counter", "my_counter"),
            counter.clone(),
        );

        counter.inc();

        let mut encoded = Vec::new();

        encode(&mut encoded, &registry);

        println!("encoded: {:?}", encoded);
    }
}
