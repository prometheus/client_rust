// Benchmark inspired by https://github.com/tikv/rust-prometheus/blob/ab1ca7285d3463504381a5025ae1951e020d6796/benches/text_encoder.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use prometheus_client::encoding::text::{encode, Encode, EncodeMetric};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
use prometheus_client::registry::Registry;
use std::io::Write;
use std::sync::atomic::AtomicU64;

pub fn text(c: &mut Criterion) {
    c.bench_function("encode", |b| {
        #[derive(Clone, Hash, PartialEq, Eq, Encode)]
        struct Labels {
            method: Method,
            status: Status,
            some_number: u64,
        }

        #[derive(Clone, Hash, PartialEq, Eq, Encode)]
        enum Method {
            Get,
            #[allow(dead_code)]
            Put,
        };

        #[derive(Clone, Hash, PartialEq, Eq)]
        enum Status {
            Two,
            #[allow(dead_code)]
            Four,
            #[allow(dead_code)]
            Five,
        };

        impl Encode for Status {
            fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
                let status = match self {
                    Status::Two => b"200",
                    Status::Four => b"400",
                    Status::Five => b"500",
                };
                writer.write_all(status)?;
                Ok(())
            }
        }

        let mut registry = Registry::<Box<dyn EncodeMetric>>::default();

        for i in 0..100 {
            let counter_family = Family::<Labels, Counter>::default();
            let histogram_family = Family::<Labels, Histogram>::new_with_constructor(|| {
                Histogram::new(exponential_buckets(1.0, 2.0, 10))
            });

            registry.register(
                format!("my_counter_{}", i),
                "My counter",
                Box::new(counter_family.clone()),
            );
            registry.register(
                format!("my_histogram_{}", i),
                "My histogram",
                Box::new(histogram_family.clone()),
            );

            for j in 0u32..100 {
                counter_family
                    .get_or_create(&Labels {
                        method: Method::Get,
                        status: Status::Two,
                        some_number: j.into(),
                    })
                    .inc();
                histogram_family
                    .get_or_create(&Labels {
                        method: Method::Get,
                        status: Status::Two,
                        some_number: j.into(),
                    })
                    .observe(j.into());
            }
        }

        let mut buffer = vec![];

        b.iter(|| {
            encode(&mut buffer, &registry).unwrap();
            black_box(&mut buffer);
        })
    });
}

criterion_group!(benches, text);
criterion_main!(benches);
