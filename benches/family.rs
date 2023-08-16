use criterion::{criterion_group, criterion_main, Criterion};
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::{CreateFromEquivalent, Equivalent, Family};

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
        }

        #[derive(Clone, Hash, PartialEq, Eq)]
        enum Status {
            Two,
            #[allow(dead_code)]
            Four,
            #[allow(dead_code)]
            Five,
        }
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

    c.bench_function(
        "counter family with custom type label set and direct lookup",
        |b| {
            #[derive(Clone, Eq, Hash, PartialEq, EncodeLabelSet)]
            struct Labels {
                method: String,
                url_path: String,
                status_code: String,
            }

            let family = Family::<Labels, Counter>::default();

            b.iter(|| {
                family
                    .get_or_create(&Labels {
                        method: "GET".to_string(),
                        url_path: "/metrics".to_string(),
                        status_code: "200".to_string(),
                    })
                    .inc();
            })
        },
    );

    c.bench_function(
        "counter family with custom type label set and equivalent lookup",
        |b| {
            #[derive(Clone, Eq, Hash, PartialEq, EncodeLabelSet)]
            struct Labels {
                method: String,
                url_path: String,
                status_code: String,
            }

            #[derive(Debug, Eq, Hash, PartialEq)]
            struct LabelsQ<'a> {
                method: &'a str,
                url_path: &'a str,
                status_code: &'a str,
            }

            impl CreateFromEquivalent<Labels> for LabelsQ<'_> {
                fn create(&self) -> Labels {
                    Labels {
                        method: self.method.to_string(),
                        url_path: self.url_path.to_string(),
                        status_code: self.status_code.to_string(),
                    }
                }
            }

            impl Equivalent<Labels> for LabelsQ<'_> {
                fn equivalent(&self, key: &Labels) -> bool {
                    self.method == key.method
                        && self.url_path == key.url_path
                        && self.status_code == key.status_code
                }
            }

            let family = Family::<Labels, Counter>::default();

            b.iter(|| {
                family
                    .get_or_create(&LabelsQ {
                        method: "GET",
                        url_path: "/metrics",
                        status_code: "200",
                    })
                    .inc();
            })
        },
    );
}

criterion_group!(benches, family);
criterion_main!(benches);
