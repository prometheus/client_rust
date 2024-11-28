//! Module implementing an Open Metrics summary metric.
//!
//! See [`Summary`] for details.

use std::fmt::Error;
use crate::encoding::{EncodeMetric, MetricEncoder};

use super::{MetricType, TypedMetric};
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};
use std::sync::Arc;

/// A marker trait for types that represent numeric values.
///
/// This trait is intended to abstract over numeric types such as integers
/// and floating-point numbers. Implementors of this trait are expected to
/// behave like numeric values in terms of operations and comparisons.
pub trait Numeric {
    /// In the definition of protobuf it is the f64 type.
    #[cfg(feature = "protobuf")]
    fn as_f64(&self) -> f64;
}
impl Numeric for u64 {
    #[cfg(feature = "protobuf")]
    fn as_f64(&self) -> f64 {
        *self as f64
    }
}
impl Numeric for i64 {
    #[cfg(feature = "protobuf")]
    fn as_f64(&self) -> f64 {
        *self as f64
    }
}
impl Numeric for f64 {
    #[cfg(feature = "protobuf")]
    fn as_f64(&self) -> f64 {
        *self
    }
}
/// Open Metrics [`Summary`] for capturing aggregate statistics, such as sum, count, and quantiles.
///
/// This implementation is flexible, allowing users to choose their own algorithms for calculating
/// quantiles. Instead of computing quantiles directly, the [`Summary`] struct assumes that users
/// will provide the quantile values as inputs.
///
/// # Example
///
/// ```
/// # use prometheus_client::metrics::summary::Summary;
/// use std::sync::Arc;
///
/// let summary = Summary::default();
/// let quantiles = vec![(0.5, 10.0), (0.9, 20.0), (0.99, 30.0)];
/// summary.reset(100.0, 42, quantiles);
///
/// let (sum, count, quantiles) = summary.get();
/// assert_eq!(sum, 100.0);
/// assert_eq!(count, 42);
/// assert_eq!(quantiles.as_slice(), &[(0.5, 10.0), (0.9, 20.0), (0.99, 30.0)]);
/// ```
///
/// # Implementation Details
///
/// - The struct internally uses an [`Arc<RwLock<Inner>>`] to allow shared and mutable access in
///   a thread-safe manner.
/// - [`Summary`] does not implement specific quantile algorithms, delegating that responsibility
///   to users. This provides flexibility for users to integrate different quantile calculation
///   methods according to their needs.
#[derive(Debug)]
pub struct Summary<T = f64>
where
    T: Numeric,
{
    inner: Arc<RwLock<Inner<T>>>,
}

impl<T> Clone for Summary<T>
where
    T: Numeric,
{
    /// Creates a new instance of [`Summary`] sharing the same underlying data.
    fn clone(&self) -> Self {
        Summary {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Inner<T> {
    /// The total sum of observed values.
    sum: f64,
    /// The count of observed values.
    count: u64,
    /// Precomputed quantile values (quantile, value).
    quantiles: Vec<(f64, T)>,
}

impl<T> Default for Summary<T>
where
    T: Numeric + Default,
{
    /// Creates a default instance of [`Summary`], initializing with zero values
    /// and a single quantile set to `(0.0, 0.0)`.
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                sum: Default::default(),
                count: Default::default(),
                quantiles: vec![(Default::default(), T::default())],
            })),
        }
    }
}
impl<T> Summary<T>
where
    T: Numeric,
{
    /// Resets the state of the [`Summary`] with new data.
    ///
    /// This method allows users to update the sum, count, and quantile values
    /// stored in the [`Summary`].
    ///
    /// # Arguments
    /// - `sum`: The new sum of observed values.
    /// - `count`: The new count of observed values.
    /// - `quantiles`: A vector of quantile values in the form of `(quantile, value)`.
    pub fn reset(&self, sum: f64, count: u64, quantiles: Vec<(f64, T)>) -> Result<(), String> {
        let quantiles_slice = &quantiles;
        for &(q, _) in quantiles_slice {
            if q < 0.0 || q > 1.0 {
                return Err(format!("Invalid quantile value: {}", q));
            }
        }

        if !quantiles_slice.windows(2).all(|w| w[0].0 <= w[1].0) {
            return Err("Quantiles must be sorted by their quantile values".to_string());
        }

        let mut inner = self.inner.write();
        inner.sum = sum;
        inner.count = count;
        inner.quantiles = quantiles;
        Ok(())
    }

    /// Retrieves the current state of the [`Summary`].
    ///
    /// Returns a tuple containing:
    /// - `sum`: The total sum of observed values.
    /// - `count`: The count of observed values.
    /// - `quantiles`: A reference to the vector of quantiles.
    pub fn get(&self) -> (f64, u64, MappedRwLockReadGuard<Vec<(f64, T)>>) {
        let inner = self.inner.read();
        let sum = inner.sum;
        let count = inner.count;
        let quantiles = RwLockReadGuard::map(inner, |inner| &inner.quantiles);
        (sum, count, quantiles)
    }
}

impl<T> TypedMetric for Summary<T>
where
    T: Numeric,
{
    /// Specifies the metric type as [`MetricType::Summary`].
    const TYPE: MetricType = MetricType::Summary;
}

impl<T> EncodeMetric for Summary<T>
where
    T: Numeric + ToString,
{
    /// Encodes the current state of the [`Summary`] into the provided `MetricEncoder`.
    ///
    /// This method delegates encoding of the `sum`, `count`, and quantiles to the encoder.
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), Error> {
        let (sum, count, quantiles) = self.get();
        encoder.encode_summary(sum, count, &quantiles)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_summary_reset_valid() {
        let summary = Summary::default();
        let quantiles = vec![(0.5, 10.0), (0.9, 20.0)];
        assert!(summary.reset(100.0, 42, quantiles).is_ok());
    }

    #[test]
    fn test_summary_reset_invalid_quantile() {
        let summary = Summary::default();
        let quantiles = vec![(1.5, 10.0), (0.9, 20.0)]; // quantile out of range
        assert!(summary.reset(100.0, 42, quantiles).is_err());
    }

    #[test]
    fn test_summary_reset_unsorted_quantiles() {
        let summary = Summary::default();
        let quantiles = vec![(0.9, 20.0), (0.5, 10.0)]; // unsorted quantile
        assert!(summary.reset(100.0, 42, quantiles).is_err());
    }

    #[test]
    fn test_summary_get() {
        let summary = Summary::default();
        let quantiles = vec![(0.5, 10.0), (0.9, 20.0)];
        summary.reset(100.0, 42, quantiles.clone()).unwrap();

        let (sum, count, q) = summary.get();
        assert_eq!(sum, 100.0);
        assert_eq!(count, 42);
        assert_eq!(q.as_slice(), &quantiles);
    }
}