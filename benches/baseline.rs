use criterion::{criterion_group, criterion_main, Criterion};
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;

pub fn baseline(c: &mut Criterion) {
    c.bench_function("counter", |b| {
        let counter: Counter = Counter::default();

        b.iter(|| {
            counter.inc();
        })
    });

    c.bench_function("counter via family lookup", |b| {
        let family = Family::<(), Counter>::default();

        b.iter(|| {
            family.get_or_create(&()).inc();
        })
    });
}

criterion_group!(benches, baseline);
criterion_main!(benches);
