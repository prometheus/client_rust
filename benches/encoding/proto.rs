// Benchmark inspired by
// https://github.com/tikv/rust-prometheus/blob/ab1ca7285d3463504381a5025ae1951e020d6796/benches/text_encoder.rs:write

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use prometheus_client::encoding::protobuf;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;
use prometheus_client_derive_encode::{EncodeLabelSet, EncodeLabelValue};

pub fn proto(c: &mut Criterion) {
    c.bench_function("encode", |b| {
        #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug)]
        struct CounterLabels {
            path: String,
            method: Method,
            some_number: u64,
        }

        #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelValue, Debug)]
        enum Method {
            Get,
            #[allow(dead_code)]
            Put,
        }

        #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug)]
        struct HistogramLabels {
            region: Region,
        }

        #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelValue, Debug)]
        enum Region {
            Africa,
            #[allow(dead_code)]
            Asia,
        }

        let mut registry = Registry::default();

        for i in 0..100 {
            let counter_family = Family::<CounterLabels, Counter>::default();
            let histogram_family =
                Family::<HistogramLabels, Histogram>::new_with_constructor(|| {
                    Histogram::new(exponential_buckets(1.0, 2.0, 10))
                });

            registry.register(
                format!("my_counter{}", i),
                "My counter",
                counter_family.clone(),
            );
            registry.register(
                format!("my_histogram{}", i),
                "My histogram",
                histogram_family.clone(),
            );

            for j in 0_u32..100 {
                counter_family
                    .get_or_create(&CounterLabels {
                        path: format!("/path/{}", i),
                        method: Method::Get,
                        some_number: j.into(),
                    })
                    .inc();

                histogram_family
                    .get_or_create(&HistogramLabels {
                        region: Region::Africa,
                    })
                    .observe(j.into());
            }
        }

        b.iter(|| {
            let metric_set = protobuf::encode(&registry).unwrap();
            black_box(metric_set);
        })
    });
}

criterion_group!(benches, proto);
criterion_main!(benches);
