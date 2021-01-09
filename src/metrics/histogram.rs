//! Module implementing an Open Metrics histogram.
//!
//! See [`Histogram`] for details.

use super::{MetricType, TypedMetric};
use generic_array::{ArrayLength, GenericArray};
use generic_array::sequence::GenericSequence;
use generic_array::typenum::U10;
use owning_ref::OwningRef;
use std::sync::{Arc, Mutex, MutexGuard};

/// Open Metrics [`Histogram`] to measure distributions of discrete events.
///
/// ```
/// # use open_metrics_client::metrics::histogram::{Histogram, exponential_series};
/// let histogram = Histogram::new(exponential_series(1.0, 2.0));
/// histogram.observe(4.2);
/// ```
// TODO: Consider using atomics. See
// https://github.com/tikv/rust-prometheus/pull/314.
pub struct Histogram<NumBuckets: ArrayLength<(f64, u64)> = U10> {
    inner: Arc<Mutex<Inner<NumBuckets>>>,
}

impl<NumBuckets: ArrayLength<(f64, u64)>> Clone for Histogram<NumBuckets> {
    fn clone(&self) -> Self {
        Histogram {
            inner: self.inner.clone(),
        }
    }
}

pub(crate) struct Inner<NumBuckets: ArrayLength<(f64, u64)>> {
    // TODO: Consider allowing integer observe values.
    sum: f64,
    count: u64,
    // TODO: Consider being generic over the bucket length.
    buckets: GenericArray<(f64, u64), NumBuckets>,
}

impl<NumBuckets: ArrayLength<(f64, u64)>> Histogram<NumBuckets> {
    pub fn new(buckets: GenericArray<(f64, u64), NumBuckets>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                sum: Default::default(),
                count: Default::default(),
                buckets,
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

    pub(crate) fn get(&self) -> (f64, u64, MutexGuardedBuckets<NumBuckets>) {
        let inner = self.inner.lock().unwrap();
        let sum = inner.sum;
        let count = inner.count;
        let buckets = OwningRef::new(inner).map(|inner| &inner.buckets);
        (sum, count, buckets)
    }
}

type MutexGuardedBuckets<'a, NumBuckets> =
    OwningRef<MutexGuard<'a, Inner<NumBuckets>>, GenericArray<(f64, u64), NumBuckets>>;

impl<NumBuckets: ArrayLength<(f64, u64)>> TypedMetric for Histogram<NumBuckets> {
    const TYPE: MetricType = MetricType::Histogram;
}

// TODO: consider renaming to exponential_buckets
pub fn exponential_series<NumBuckets: ArrayLength<(f64, u64)>>(
    start: f64,
    factor: f64,
) -> GenericArray<(f64, u64), NumBuckets> {
    GenericArray::generate(|i: usize| (start * factor.powf(i as f64), 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram() {
        let histogram = Histogram::<U10>::new(exponential_series(1.0, 2.0));
        histogram.observe(1.0);
    }
}
