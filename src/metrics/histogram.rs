//! Module implementing an Open Metrics histogram.
//!
//! See [`Histogram`] for details.

use super::{MetricType, TypedMetric};
use owning_ref::OwningRef;
use std::iter::once;
use std::sync::{Arc, Mutex, MutexGuard};

/// Open Metrics [`Histogram`] to measure distributions of discrete events.
///
/// ```
/// # use open_metrics_client::metrics::histogram::{Histogram, exponential_series};
/// let histogram = Histogram::new(exponential_series(1.0, 2.0, 10));
/// histogram.observe(4.2);
/// ```
// TODO: Consider using atomics. See
// https://github.com/tikv/rust-prometheus/pull/314.
pub struct Histogram {
    inner: Arc<Mutex<Inner>>,
}

impl Clone for Histogram {
    fn clone(&self) -> Self {
        Histogram {
            inner: self.inner.clone(),
        }
    }
}

pub(crate) struct Inner {
    // TODO: Consider allowing integer observe values.
    sum: f64,
    count: u64,
    // TODO: Consider being generic over the bucket length.
    buckets: Vec<(f64, u64)>,
}

impl Histogram {
    pub fn new(buckets: impl Iterator<Item = f64>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                sum: Default::default(),
                count: Default::default(),
                buckets: buckets
                    .into_iter()
                    .chain(once(f64::MAX))
                    .map(|upper_bound| (upper_bound, 0))
                    .collect(),
            })),
        }
    }

    pub fn observe(&self, v: f64) {
        let mut inner = self.inner.lock().unwrap();
        inner.sum += v;
        inner.count += 1;
        for (upper_bound, count) in inner.buckets.iter_mut() {
            if *upper_bound >= v {
                *count += 1;
            }
        }
    }

    pub(crate) fn get(&self) -> (f64, u64, MutexGuardedBuckets) {
        let inner = self.inner.lock().unwrap();
        let sum = inner.sum;
        let count = inner.count;
        let buckets = OwningRef::new(inner).map(|inner| &inner.buckets);
        (sum, count, buckets)
    }
}

type MutexGuardedBuckets<'a> = OwningRef<MutexGuard<'a, Inner>, Vec<(f64, u64)>>;

impl TypedMetric for Histogram {
    const TYPE: MetricType = MetricType::Histogram;
}

// TODO: consider renaming to exponential_buckets
pub fn exponential_series(start: f64, factor: f64, length: u16) -> impl Iterator<Item = f64> {
    let mut current = start;
    (0..length).map(move |_| {
        let to_return = current;
        current *= factor;
        to_return
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram() {
        let histogram = Histogram::new(exponential_series(1.0, 2.0, 10));
        histogram.observe(1.0);
    }
}
