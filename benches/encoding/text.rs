// Benchmark inspired by https://github.com/tikv/rust-prometheus/blob/ab1ca7285d3463504381a5025ae1951e020d6796/benches/text_encoder.rs

use criterion::{criterion_group, criterion_main, Criterion};
use prometheus_client::encoding::{self, EncodeLabelSet, EncodeLabelValue, LabelValueEncoder};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;
use std::fmt::Write;
use std::hint::black_box;

pub fn text(c: &mut Criterion) {
    bench_text(c, "encode", "200");
    bench_text(c, "encode_escaped_label_values", "2\\0\"0\n");
}

fn bench_text(c: &mut Criterion, name: &str, status_value: &'static str) {
    c.bench_function(name, |b| {
        #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug)]
        struct Labels {
            method: Method,
            status: Status,
            some_number: u64,
        }

        #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelValue, Debug)]
        enum Method {
            Get,
            #[allow(dead_code)]
            Put,
        }

        #[derive(Clone, Hash, PartialEq, Eq, Debug)]
        struct Status(&'static str);

        impl prometheus_client::encoding::EncodeLabelValue for Status {
            fn encode(&self, writer: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
                writer.write_str(self.0)
            }
        }

        let mut registry = Registry::default();

        for i in 0..100 {
            let counter_family = Family::<Labels, Counter>::default();
            let histogram_family = Family::<Labels, Histogram>::new_with_constructor(|| {
                Histogram::new(exponential_buckets(1.0, 2.0, 10))
            });

            registry.register(
                format!("my_counter_{i}"),
                "My counter",
                counter_family.clone(),
            );
            registry.register(
                format!("my_histogram_{i}"),
                "My histogram",
                histogram_family.clone(),
            );

            for j in 0u32..100 {
                counter_family
                    .get_or_create(&Labels {
                        method: Method::Get,
                        status: Status(status_value),
                        some_number: j.into(),
                    })
                    .inc();
                histogram_family
                    .get_or_create(&Labels {
                        method: Method::Get,
                        status: Status(status_value),
                        some_number: j.into(),
                    })
                    .observe(j.into());
            }
        }

        let mut buffer = String::new();

        b.iter(|| {
            buffer.clear();
            encoding::text::encode(&mut buffer, &registry).unwrap();
            black_box(&mut buffer);
        })
    });
}

criterion_group!(benches, text);
criterion_main!(benches);
