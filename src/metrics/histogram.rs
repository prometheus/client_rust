//! Module implementing an Open Metrics histogram.
//!
//! See [`Histogram`] for details.

use super::{MetricType, TypedMetric};
use owning_ref::OwningRef;
use std::iter::{self, once};
use std::sync::{Arc, Mutex, MutexGuard};

/// Open Metrics [`Histogram`] to measure distributions of discrete events.
///
/// ```
/// # use prometheus_client::metrics::histogram::{Histogram, exponential_buckets};
/// let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
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
        self.observe_and_bucket(v);
    }

    /// Observes the given value, returning the index of the first bucket the
    /// value is added to.
    ///
    /// Needed in
    /// [`HistogramWithExemplars`](crate::metrics::exemplar::HistogramWithExemplars).
    pub(crate) fn observe_and_bucket(&self, v: f64) -> Option<usize> {
        let mut inner = self.inner.lock().unwrap();
        inner.sum += v;
        inner.count += 1;

        let first_bucket = inner
            .buckets
            .iter_mut()
            .enumerate()
            .find(|(_i, (upper_bound, _value))| upper_bound >= &v);

        match first_bucket {
            Some((i, (_upper_bound, value))) => {
                *value += 1;
                Some(i)
            }
            None => None,
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

pub(crate) type MutexGuardedBuckets<'a> = OwningRef<MutexGuard<'a, Inner>, Vec<(f64, u64)>>;

impl TypedMetric for Histogram {
    const TYPE: MetricType = MetricType::Histogram;
}

pub fn exponential_buckets(start: f64, factor: f64, length: u16) -> impl Iterator<Item = f64> {
    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| start * factor.powf(i as f64))
        .take(length.into())
}

pub fn linear_buckets(start: f64, width: f64, length: u16) -> impl Iterator<Item = f64> {
    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| start + (width * (i as f64)))
        .take(length.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram() {
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        histogram.observe(1.0);
    }

    #[test]
    fn exponential() {
        assert_eq!(
            vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0],
            exponential_buckets(1.0, 2.0, 10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn linear() {
        assert_eq!(
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
            linear_buckets(0.0, 1.0, 10).collect::<Vec<_>>()
        );
    }
}
