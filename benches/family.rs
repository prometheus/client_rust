use criterion::{black_box, criterion_group, criterion_main, Criterion};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;

pub fn family(c: &mut Criterion) {
    c.bench_function("counter family with Vec<(String, String)> label set", |b| {
        let family = Family::<Vec<(String, String)>, Counter>::default();

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
        let family = Family::<Labels, Counter>::default();

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
