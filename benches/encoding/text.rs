// Benchmark inspired by https://github.com/tikv/rust-prometheus/blob/ab1ca7285d3463504381a5025ae1951e020d6796/benches/text_encoder.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use open_metrics_client::counter::Counter;
use open_metrics_client::encoding::text::{encode, Encode, EncodeMetric};
use open_metrics_client::family::Family;
use open_metrics_client::histogram::Histogram;
use open_metrics_client::registry::{Descriptor, Registry};
use std::io::Write;
use std::sync::atomic::AtomicU64;

pub fn text(c: &mut Criterion) {
    c.bench_function("encode", |b| {
        #[derive(Clone, Hash, PartialEq, Eq)]
        struct Labels {
            method: Method,
            status: Status,
            some_number: u64,
        }

        #[derive(Clone, Hash, PartialEq, Eq)]
        enum Method {
            Get,
            Put,
        };

        #[derive(Clone, Hash, PartialEq, Eq)]
        enum Status {
            Two,
            Four,
            Five,
        };

        impl Encode for Labels {
            fn encode(&self, mut writer: &mut dyn Write) -> Result<(), std::io::Error> {
                let method = match self.method {
                    Method::Get => b"method=\"GET\"",
                    Method::Put => b"method=\"PUT\"",
                };
                writer.write_all(method)?;

                writer.write_all(b", ")?;
                let status = match self.status {
                    Status::Two => b"status=\"200\"",
                    Status::Four => b"status=\"400\"",
                    Status::Five => b"status=\"500\"",
                };
                writer.write_all(status)?;

                writer.write_all(b", ")?;
                writer.write_all(b"some_number=\"")?;
                self.some_number.encode(&mut writer)?;
                writer.write_all(b"\"")?;
                Ok(())
            }
        }

        let mut registry = Registry::<Box<dyn EncodeMetric>>::new();

        for i in 0..100 {
            let counter_family = Family::<Labels, Counter<AtomicU64>>::new();
            let histogram_family = Family::<Labels, Histogram>::new();

            registry.register(
                Descriptor::new(
                    "Counter".to_string(),
                    "My counter".to_string(),
                    format!("my_counter_{}", i),
                ),
                Box::new(counter_family.clone()),
            );
            registry.register(
                Descriptor::new(
                    "Histogram".to_string(),
                    "My histogram".to_string(),
                    format!("my_histogram_{}", i),
                ),
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
