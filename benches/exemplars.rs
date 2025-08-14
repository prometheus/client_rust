use std::time::SystemTime;

use criterion::{criterion_group, criterion_main, Criterion};
use prometheus_client::metrics::exemplar::HistogramWithExemplars;
use prometheus_client::metrics::histogram::Histogram;

type Exemplar = Vec<(String, String)>;

const BUCKETS: &[f64] = &[1.0, 2.0, 3.0];

pub fn exemplars(c: &mut Criterion) {
    c.bench_function("histogram without exemplars", |b| {
        let histogram = Histogram::new(BUCKETS.iter().copied());

        b.iter(|| {
            histogram.observe(1.0);
        });
    });

    c.bench_function("histogram with exemplars (no exemplar passed)", |b| {
        let histogram = HistogramWithExemplars::<Exemplar>::new(BUCKETS.iter().copied());

        b.iter(|| {
            histogram.observe(1.0, None, None);
        });
    });

    c.bench_function("histogram with exemplars (some exemplar passed)", |b| {
        let histogram = HistogramWithExemplars::<Exemplar>::new(BUCKETS.iter().copied());
        let exemplar = vec![("TraceID".to_owned(), "deadfeed".to_owned())];

        b.iter(|| {
            histogram.observe(1.0, Some(exemplar.clone()), Some(SystemTime::now()));
        });
    });
}

criterion_group!(benches, exemplars);
criterion_main!(benches);
