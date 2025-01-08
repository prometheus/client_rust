//! Module implementing an Open Metrics histogram.
//!
//! See [`Histogram`] for details.

use crate::encoding::{EncodeMetric, MetricEncoder, NoLabelSet};

use super::{MetricType, TypedMetric};
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};
use std::iter::{self, once};
use std::sync::Arc;

/// Open Metrics [`Histogram`] to measure distributions of discrete events.
///
/// ```
/// # use prometheus_client::metrics::histogram::{Histogram, exponential_buckets};
/// let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
/// histogram.observe(4.2);
/// ```
///
/// [`Histogram`] does not implement [`Default`], given that the choice of
/// bucket values depends on the situation [`Histogram`] is used in. As an
/// example, to measure HTTP request latency, the values suggested in the
/// Golang implementation might work for you:
///
/// ```
/// # use prometheus_client::metrics::histogram::Histogram;
/// // Default values from go client(https://github.com/prometheus/client_golang/blob/5d584e2717ef525673736d72cd1d12e304f243d7/prometheus/histogram.go#L68)
/// let custom_buckets = [
///    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
/// ];
/// let histogram = Histogram::new(custom_buckets);
/// histogram.observe(4.2);
/// ```
// TODO: Consider using atomics. See
// https://github.com/tikv/rust-prometheus/pull/314.
#[derive(Debug)]
pub struct Histogram {
    inner: Arc<RwLock<Inner>>,
}

impl Clone for Histogram {
    fn clone(&self) -> Self {
        Histogram {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Inner {
    // TODO: Consider allowing integer observe values.
    sum: f64,
    count: u64,
    // TODO: Consider being generic over the bucket length.
    buckets: Vec<(f64, u64)>,
}

impl Histogram {
    /// Create a new [`Histogram`].
    ///
    /// ```rust
    /// # use prometheus_client::metrics::histogram::Histogram;
    /// let histogram = Histogram::new([10.0, 100.0, 1_000.0]);
    /// ```
    pub fn new(buckets: impl IntoIterator<Item = f64>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
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

    /// Observe the given value.
    pub fn observe(&self, v: f64) {
        self.observe_and_bucket(v);
    }

    /// Observes the given value, returning the index of the first bucket the
    /// value is added to.
    ///
    /// Needed in
    /// [`HistogramWithExemplars`](crate::metrics::exemplar::HistogramWithExemplars).
    pub(crate) fn observe_and_bucket(&self, v: f64) -> Option<usize> {
        let mut inner = self.inner.write();
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

    pub(crate) fn get(&self) -> (f64, u64, MappedRwLockReadGuard<Vec<(f64, u64)>>) {
        let inner = self.inner.read();
        let sum = inner.sum;
        let count = inner.count;
        let buckets = RwLockReadGuard::map(inner, |inner| &inner.buckets);
        (sum, count, buckets)
    }
}

impl TypedMetric for Histogram {
    const TYPE: MetricType = MetricType::Histogram;
}

/// Exponential bucket distribution.
pub fn exponential_buckets(start: f64, factor: f64, length: u16) -> impl Iterator<Item = f64> {
    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| start * factor.powf(i as f64))
        .take(length.into())
}

/// Exponential bucket distribution within a range
///
/// Creates `length` buckets, where the lowest bucket is `min` and the highest bucket is `max`.
///
/// If `length` is less than 1, or `min` is less than or equal to 0, an empty iterator is returned.
pub fn exponential_buckets_range(min: f64, max: f64, length: u16) -> impl Iterator<Item = f64> {
    let mut len_observed = length;
    let mut min_bucket = min;
    // length needs a positive length and min needs to be greater than 0
    // set len_observed to 0 and min_bucket to 1.0
    // this will return an empty iterator in the result
    if length < 1 || min <= 0.0 {
        len_observed = 0;
        min_bucket = 1.0;
    }
    // We know max/min and highest bucket. Solve for growth_factor.
    let growth_factor = (max / min_bucket).powf(1.0 / (len_observed as f64 - 1.0));

    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| min_bucket * growth_factor.powf(i as f64))
        .take(len_observed.into())
}

/// Linear bucket distribution.
pub fn linear_buckets(start: f64, width: f64, length: u16) -> impl Iterator<Item = f64> {
    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| start + (width * (i as f64)))
        .take(length.into())
}

impl EncodeMetric for Histogram {
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        let (sum, count, buckets) = self.get();
        encoder.encode_histogram::<NoLabelSet>(sum, count, &buckets, None)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
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

    #[test]
    fn exponential_range() {
        assert_eq!(
            vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0],
            exponential_buckets_range(1.0, 32.0, 6).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exponential_range_incorrect() {
        let res = exponential_buckets_range(1.0, 32.0, 0).collect::<Vec<_>>();
        assert!(res.is_empty());

        let res = exponential_buckets_range(0.0, 32.0, 6).collect::<Vec<_>>();
        assert!(res.is_empty());
    }
}
