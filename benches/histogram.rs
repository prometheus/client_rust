use criterion::{black_box, criterion_group, criterion_main, Criterion};
use prometheus_client::metrics::histogram::{
    exponential_buckets, Histogram, NativeHistogramConfig,
};

const OBSERVATION: f64 = 64.0;

pub fn histogram(c: &mut Criterion) {
    let mut group = c.benchmark_group("observe");

    group.bench_function("histogram", |b| {
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));

        b.iter(|| {
            histogram.observe(black_box(OBSERVATION));
        })
    });

    group.bench_function("native histogram", |b| {
        let histogram = Histogram::new_native(NativeHistogramConfig::with_schema(0));

        b.iter(|| {
            histogram.observe(black_box(OBSERVATION));
        })
    });

    group.bench_function("classic and native histogram", |b| {
        let histogram = Histogram::new_classic_and_native(
            exponential_buckets(1.0, 2.0, 10),
            NativeHistogramConfig::with_schema(0),
        );

        b.iter(|| {
            histogram.observe(black_box(OBSERVATION));
        })
    });

    group.finish();
}

criterion_group!(benches, histogram);
criterion_main!(benches);
