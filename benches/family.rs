use std::time::Instant;

use criterion::{criterion_group, criterion_main, Criterion};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;

pub fn family(c: &mut Criterion) {
    c.bench_function(
        "counter family with Vec<(String, String)> fixed label set (get_or_create)",
        |b| {
            let family = Family::<Vec<(String, String)>, Counter>::default();

            b.iter(|| {
                family
                    .get_or_create(&vec![
                        ("method".to_owned(), "GET".to_owned()),
                        ("status".to_owned(), "200".to_owned()),
                    ])
                    .inc();
            });
        },
    );

    c.bench_function(
        "counter family with Vec<(String, String)> fixed label set (with_labels)",
        |b| {
            let family = Family::<Vec<(String, String)>, Counter>::default();

            b.iter(|| {
                family.with_labels(
                    vec![
                        ("method".to_owned(), "GET".to_owned()),
                        ("status".to_owned(), "200".to_owned()),
                    ],
                    |counter| counter.inc(),
                )
            })
        },
    );

    c.bench_function(
        "counter family with Vec<(String, String)> dynamic label set (get_or_create)",
        |b| {
            let family = Family::<Vec<(String, String)>, Counter>::default();

            b.iter_custom(|iters| {
                let start = Instant::now();
                for i in 0..iters {
                    family
                        .get_or_create(&vec![
                            ("method".to_owned(), "GET".to_owned()),
                            ("status".to_owned(), (i % 100).to_string()),
                        ])
                        .inc();
                }
                start.elapsed()
            });
        },
    );

    c.bench_function(
        "counter family with Vec<(String, String)> dynamic label set (with_labels)",
        |b| {
            let family = Family::<Vec<(String, String)>, Counter>::default();

            b.iter_custom(|iters| {
                let start = Instant::now();
                for i in 0..iters {
                    family.with_labels(
                        vec![
                            ("method".to_owned(), "GET".to_owned()),
                            ("status".to_owned(), (i % 100).to_string()),
                        ],
                        |counter| counter.inc(),
                    );
                }
                start.elapsed()
            });
        },
    );

    #[derive(Clone, Hash, PartialEq, Eq)]
    struct Labels {
        method: Method,
        status: Status,
    }

    #[derive(Clone, Hash, PartialEq, Eq)]
    enum Method {
        Get,
        #[allow(dead_code)]
        Put,
    }

    #[derive(Clone, Hash, PartialEq, Eq)]
    enum Status {
        Two,
        #[allow(dead_code)]
        Four,
        #[allow(dead_code)]
        Five,
    }

    c.bench_function(
        "counter family with custom type label set (get_or_create)",
        |b| {
            let family = Family::<Labels, Counter>::default();

            b.iter(|| {
                family
                    .get_or_create(&Labels {
                        method: Method::Get,
                        status: Status::Two,
                    })
                    .inc();
            })
        },
    );

    c.bench_function(
        "counter family with custom type label set (with_labels)",
        |b| {
            let family = Family::<Labels, Counter>::default();

            b.iter(|| {
                family.with_labels(
                    Labels {
                        method: Method::Get,
                        status: Status::Two,
                    },
                    |counter| counter.inc(),
                );
            })
        },
    );
}

criterion_group!(benches, family);
criterion_main!(benches);
