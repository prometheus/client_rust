// Benchmark inspired by
// https://github.com/tikv/rust-prometheus/blob/ab1ca7285d3463504381a5025ae1951e020d6796/benches/text_encoder.rs:write

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use prometheus_client::encoding::proto::{encode, EncodeMetric};
use prometheus_client::encoding::Encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;
use std::fmt::{Display, Formatter};

pub fn proto(c: &mut Criterion) {
    c.bench_function("encode", |b| {
        #[derive(Clone, Hash, PartialEq, Eq, Encode)]
        struct Labels {
            path: String,
            method: Method,
            some_number: u64,
        }

        #[derive(Clone, Hash, PartialEq, Eq, Encode)]
        enum Method {
            Get,
            #[allow(dead_code)]
            Put,
        }

        impl Display for Method {
            fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
                match self {
                    Method::Get => write!(f, "Get"),
                    Method::Put => write!(f, "Put"),
                }
            }
        }

        #[derive(Clone, Hash, PartialEq, Eq, Encode)]
        enum Region {
            Africa,
            #[allow(dead_code)]
            Asia,
        }

        let mut registry = Registry::<Box<dyn EncodeMetric>>::default();

        for i in 0..100 {
            let counter_family = Family::<Labels, Counter>::default();
            let histogram_family = Family::<Region, Histogram>::new_with_constructor(|| {
                Histogram::new(exponential_buckets(1.0, 2.0, 10))
            });

            registry.register(
                format!("my_counter{}", i),
                "My counter",
                Box::new(counter_family.clone()),
            );
            registry.register(
                format!("my_histogram{}", i),
                "My histogram",
                Box::new(histogram_family.clone()),
            );

            for j in 0_u32..100 {
                counter_family
                    .get_or_create(&Labels {
                        path: format!("/path/{}", i),
                        method: Method::Get,
                        some_number: j.into(),
                    })
                    .inc();

                histogram_family
                    .get_or_create(&Region::Africa)
                    .observe(j.into());
            }
        }

        b.iter(|| {
            let metric_set = encode(&registry);
            black_box(metric_set);
        })
    });
}

criterion_group!(benches, proto);
criterion_main!(benches);
