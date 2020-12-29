use criterion::{black_box, criterion_group, criterion_main, Criterion};
use open_metrics_client::counter::Counter;
use open_metrics_client::family::MetricFamily;
use std::sync::atomic::AtomicU64;

pub fn family(c: &mut Criterion) {
    c.bench_function("counter family with Vec<(String, String)> label set", |b| {
        let family = MetricFamily::<Vec<(String, String)>, Counter<AtomicU64>>::new();

        b.iter(|| {
            family
                .get_or_create(&vec![
                    ("method".to_owned(), "GET".to_owned()),
                    ("status".to_owned(), "200".to_owned()),
                ])
                .inc();
        })
    });

    c.bench_function("counter family with custom type label set", |b| {
        #[derive(Clone, Hash, PartialEq, Eq)]
        struct Labels {
            method: Method,
            status: Status,
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
        let family = MetricFamily::<Labels, Counter<AtomicU64>>::new();

        b.iter(|| {
            family
                .get_or_create(&Labels {
                    method: Method::Get,
                    status: Status::Two,
                })
                .inc();
        })
    });
}

criterion_group!(benches, family);
criterion_main!(benches);
