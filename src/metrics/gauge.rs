//! Module implementing an Open Metrics gauge.
//!
//! See [`Gauge`] for details.

use crate::encoding::{EncodeGaugeValue, EncodeMetric, MetricEncoder};

use super::{MetricType, TypedMetric};
use std::marker::PhantomData;
use std::sync::atomic::{AtomicI32, Ordering};
#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
use std::sync::atomic::{AtomicI64, AtomicU64};
use std::sync::Arc;

/// Open Metrics [`Gauge`] to record current measurements.
///
/// Single increasing, decreasing or constant value metric.
///
/// [`Gauge`] is generic over the actual data type tracking the [`Gauge`] state
/// as well as the data type used to interact with the [`Gauge`]. Out of
/// convenience the generic type parameters are set to use an [`AtomicI64`] as a
/// storage and [`i64`] on the interface by default.
///
/// # Examples
///
/// ## Using [`AtomicI64`] as storage and [`i64`] on the interface
///
/// ```
/// # use prometheus_client::metrics::gauge::Gauge;
/// let gauge: Gauge = Gauge::default();
/// gauge.set(42);
/// let _value = gauge.get();
/// ```
///
/// ## Using [`AtomicU64`] as storage and [`f64`] on the interface
///
/// ```
/// # use prometheus_client::metrics::gauge::Gauge;
/// # use std::sync::atomic::AtomicU64;
/// let gauge = Gauge::<f64, AtomicU64>::default();
/// gauge.set(42.0);
/// let _value: f64 = gauge.get();
/// ```
#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
#[derive(Debug)]
pub struct Gauge<N = i64, A = AtomicI64> {
    value: Arc<A>,
    phantom: PhantomData<N>,
}

/// Open Metrics [`Gauge`] to record current measurements.
#[cfg(any(target_arch = "mips", target_arch = "powerpc"))]
#[derive(Debug)]
pub struct Gauge<N = i32, A = AtomicI32> {
    value: Arc<A>,
    phantom: PhantomData<N>,
}

impl<N, A> Clone for Gauge<N, A> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            phantom: PhantomData,
        }
    }
}

impl<N, A: Default> Default for Gauge<N, A> {
    fn default() -> Self {
        Self {
            value: Arc::new(A::default()),
            phantom: PhantomData,
        }
    }
}

impl<N, A: Atomic<N>> Gauge<N, A> {
    /// Increase the [`Gauge`] by 1, returning the previous value.
    pub fn inc(&self) -> N {
        self.value.inc()
    }

    /// Increase the [`Gauge`] by `v`, returning the previous value.
    pub fn inc_by(&self, v: N) -> N {
        self.value.inc_by(v)
    }

    /// Decrease the [`Gauge`] by 1, returning the previous value.
    pub fn dec(&self) -> N {
        self.value.dec()
    }

    /// Decrease the [`Gauge`] by `v`, returning the previous value.
    pub fn dec_by(&self, v: N) -> N {
        self.value.dec_by(v)
    }

    /// Sets the [`Gauge`] to `v`, returning the previous value.
    pub fn set(&self, v: N) -> N {
        self.value.set(v)
    }

    /// Get the current value of the [`Gauge`].
    pub fn get(&self) -> N {
        self.value.get()
    }

    /// Exposes the inner atomic type of the [`Gauge`].
    ///
    /// This should only be used for advanced use-cases which are not directly
    /// supported by the library.
    pub fn inner(&self) -> &A {
        &self.value
    }
}

/// Atomic operations for a [`Gauge`] value store.
pub trait Atomic<N> {
    /// Increase the value by `1`.
    fn inc(&self) -> N;

    /// Increase the value.
    fn inc_by(&self, v: N) -> N;

    /// Decrease the value by `1`.
    fn dec(&self) -> N;

    /// Decrease the value.
    fn dec_by(&self, v: N) -> N;

    /// Set the value.
    fn set(&self, v: N) -> N;

    /// Get the value.
    fn get(&self) -> N;
}

#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
impl Atomic<i64> for AtomicI64 {
    fn inc(&self) -> i64 {
        self.inc_by(1)
    }

    fn inc_by(&self, v: i64) -> i64 {
        self.fetch_add(v, Ordering::Relaxed)
    }

    fn dec(&self) -> i64 {
        self.dec_by(1)
    }

    fn dec_by(&self, v: i64) -> i64 {
        self.fetch_sub(v, Ordering::Relaxed)
    }

    fn set(&self, v: i64) -> i64 {
        self.swap(v, Ordering::Relaxed)
    }

    fn get(&self) -> i64 {
        self.load(Ordering::Relaxed)
    }
}

impl Atomic<i32> for AtomicI32 {
    fn inc(&self) -> i32 {
        self.inc_by(1)
    }

    fn inc_by(&self, v: i32) -> i32 {
        self.fetch_add(v, Ordering::Relaxed)
    }

    fn dec(&self) -> i32 {
        self.dec_by(1)
    }

    fn dec_by(&self, v: i32) -> i32 {
        self.fetch_sub(v, Ordering::Relaxed)
    }

    fn set(&self, v: i32) -> i32 {
        self.swap(v, Ordering::Relaxed)
    }

    fn get(&self) -> i32 {
        self.load(Ordering::Relaxed)
    }
}

#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
impl Atomic<f64> for AtomicU64 {
    fn inc(&self) -> f64 {
        self.inc_by(1.0)
    }

    fn inc_by(&self, v: f64) -> f64 {
        let mut old_u64 = self.load(Ordering::Relaxed);
        let mut old_f64;
        loop {
            old_f64 = f64::from_bits(old_u64);
            let new = f64::to_bits(old_f64 + v);
            match self.compare_exchange_weak(old_u64, new, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(x) => old_u64 = x,
            }
        }

        old_f64
    }

    fn dec(&self) -> f64 {
        self.dec_by(1.0)
    }

    fn dec_by(&self, v: f64) -> f64 {
        let mut old_u64 = self.load(Ordering::Relaxed);
        let mut old_f64;
        loop {
            old_f64 = f64::from_bits(old_u64);
            let new = f64::to_bits(old_f64 - v);
            match self.compare_exchange_weak(old_u64, new, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(x) => old_u64 = x,
            }
        }

        old_f64
    }

    fn set(&self, v: f64) -> f64 {
        f64::from_bits(self.swap(f64::to_bits(v), Ordering::Relaxed))
    }

    fn get(&self) -> f64 {
        f64::from_bits(self.load(Ordering::Relaxed))
    }
}

impl<N, A> TypedMetric for Gauge<N, A> {
    const TYPE: MetricType = MetricType::Gauge;
}

impl<N, A> EncodeMetric for Gauge<N, A>
where
    N: EncodeGaugeValue,
    A: Atomic<N>,
{
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_gauge(&self.get())
    }
    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

/// As a [`Gauge`], but constant, meaning it cannot change once created.
///
/// Needed for advanced use-cases, e.g. in combination with [`Collector`](crate::collector::Collector).
#[derive(Debug, Default)]
pub struct ConstGauge<N = i64> {
    value: N,
}

impl<N> ConstGauge<N> {
    /// Creates a new [`ConstGauge`].
    pub fn new(value: N) -> Self {
        Self { value }
    }
}

impl<N> TypedMetric for ConstGauge<N> {
    const TYPE: MetricType = MetricType::Gauge;
}

impl<N> EncodeMetric for ConstGauge<N>
where
    N: EncodeGaugeValue,
{
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_gauge(&self.value)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inc_dec_and_get() {
        let gauge: Gauge = Gauge::default();
        assert_eq!(0, gauge.inc());
        assert_eq!(1, gauge.get());

        assert_eq!(1, gauge.dec());
        assert_eq!(0, gauge.get());

        assert_eq!(0, gauge.set(10));
        assert_eq!(10, gauge.get());
    }
}
