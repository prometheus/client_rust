//! Module implementing an Open Metrics counter.
//!
//! See [`Counter`] for details.

use super::{MetricType, TypedMetric};
use std::marker::PhantomData;
#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Open Metrics [`Counter`] to measure discrete events.
///
/// Single monotonically increasing value metric.
///
/// [`Counter`] is generic over the actual data type tracking the [`Counter`]
/// state as well as the data type used to interact with the [`Counter`]. Out of
/// convenience the generic type parameters are set to use an [`AtomicU64`] as a
/// storage and [`u64`] on the interface by default.
///
/// # Examples
///
/// ## Using [`AtomicU64`] as storage and [`u64`] on the interface
///
/// ```
/// # use prometheus_client::metrics::counter::Counter;
/// let counter: Counter = Counter::default();
/// counter.inc();
/// let _value: u64 = counter.get();
/// ```
///
/// ## Using [`AtomicU64`] as storage and [`f64`] on the interface
///
/// ```
/// # use prometheus_client::metrics::counter::Counter;
/// # use std::sync::atomic::AtomicU64;
/// let counter = Counter::<f64, AtomicU64>::default();
/// counter.inc();
/// let _value: f64 = counter.get();
/// ```
#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
pub struct Counter<N = u64, A = AtomicU64> {
    value: Arc<A>,
    phantom: PhantomData<N>,
}

#[cfg(any(target_arch = "mips", target_arch = "powerpc"))]
pub struct Counter<N = u32, A = AtomicU32> {
    value: Arc<A>,
    phantom: PhantomData<N>,
}

impl<N, A> Clone for Counter<N, A> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            phantom: PhantomData,
        }
    }
}

impl<N, A: Default> Default for Counter<N, A> {
    fn default() -> Self {
        Counter {
            value: Arc::new(A::default()),
            phantom: PhantomData,
        }
    }
}

impl<N, A: Atomic<N>> Counter<N, A> {
    /// Increase the [`Counter`] by 1, returning the previous value.
    pub fn inc(&self) -> N {
        self.value.inc()
    }

    /// Increase the [`Counter`] by `v`, returning the previous value.
    pub fn inc_by(&self, v: N) -> N {
        self.value.inc_by(v)
    }

    /// Get the current value of the [`Counter`].
    pub fn get(&self) -> N {
        self.value.get()
    }

    /// Exposes the inner atomic type of the [`Counter`].
    ///
    /// This should only be used for advanced use-cases which are not directly
    /// supported by the library.
    ///
    /// The caller of this function has to uphold the property of an Open
    /// Metrics counter namely that the value is monotonically increasing, i.e.
    /// either stays the same or increases.
    pub fn inner(&self) -> &A {
        &self.value
    }
}

pub trait Atomic<N> {
    fn inc(&self) -> N;

    fn inc_by(&self, v: N) -> N;

    fn get(&self) -> N;
}

#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
impl Atomic<u64> for AtomicU64 {
    fn inc(&self) -> u64 {
        self.inc_by(1)
    }

    fn inc_by(&self, v: u64) -> u64 {
        self.fetch_add(v, Ordering::Relaxed)
    }

    fn get(&self) -> u64 {
        self.load(Ordering::Relaxed)
    }
}

impl Atomic<u32> for AtomicU32 {
    fn inc(&self) -> u32 {
        self.inc_by(1)
    }

    fn inc_by(&self, v: u32) -> u32 {
        self.fetch_add(v, Ordering::Relaxed)
    }

    fn get(&self) -> u32 {
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

    fn get(&self) -> f64 {
        f64::from_bits(self.load(Ordering::Relaxed))
    }
}

impl<N, A> TypedMetric for Counter<N, A> {
    const TYPE: MetricType = MetricType::Counter;
}

#[cfg(test)]
mod tests {
    use super::*;
    use quickcheck::QuickCheck;

    #[test]
    fn inc_and_get() {
        let counter: Counter = Counter::default();
        assert_eq!(0, counter.inc());
        assert_eq!(1, counter.get());
    }

    #[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
    #[test]
    fn f64_stored_in_atomic_u64() {
        fn prop(fs: Vec<f64>) {
            let fs: Vec<f64> = fs
                .into_iter()
                // Map infinite, subnormal and NaN to 0.0.
                .map(|f| if f.is_normal() { f } else { 0.0 })
                .collect();
            let sum = fs.iter().sum();
            let counter = Counter::<f64, AtomicU64>::default();
            for f in fs {
                counter.inc_by(f);
            }
            assert_eq!(counter.get(), sum)
        }

        QuickCheck::new().tests(10).quickcheck(prop as fn(_))
    }
}
