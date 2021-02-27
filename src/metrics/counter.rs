//! Module implementing an Open Metrics counter.
//!
//! See [`Counter`] for details.

use super::{MetricType, TypedMetric};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Open Metrics [`Counter`] to measure discrete events.
///
/// Single monotonically increasing value metric.
///
/// [`Counter`] is generic over the actual data type tracking the counter state.
/// Out of convenience the generic type parameter is set to use an [`AtomicU64`]
/// by default.
///
/// ```
/// # use open_metrics_client::metrics::counter::Counter;
/// let counter: Counter = Counter::default();
/// counter.inc();
/// ```
pub struct Counter<A = AtomicU64> {
    value: Arc<A>,
}

impl<A> Clone for Counter<A> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

impl<A: Default> Default for Counter<A> {
    fn default() -> Self {
        Counter {
            value: Arc::new(A::default()),
        }
    }
}

impl<A: Atomic> Counter<A> {
    pub fn inc(&self) -> A::Number {
        self.value.inc()
    }

    pub fn inc_by(&self, v: A::Number) -> A::Number {
        self.value.inc_by(v)
    }

    pub fn get(&self) -> A::Number {
        self.value.get()
    }

    /// Exposes the inner atomic type.
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

pub trait Atomic {
    type Number;

    fn inc(&self) -> Self::Number;

    fn inc_by(&self, v: Self::Number) -> Self::Number;

    fn get(&self) -> Self::Number;
}

impl Atomic for AtomicU64 {
    type Number = u64;

    fn inc(&self) -> Self::Number {
        self.inc_by(1)
    }

    fn inc_by(&self, v: Self::Number) -> Self::Number {
        self.fetch_add(v, Ordering::Relaxed)
    }

    fn get(&self) -> Self::Number {
        self.load(Ordering::Relaxed)
    }
}

impl Atomic for AtomicU32 {
    type Number = u32;

    fn inc(&self) -> Self::Number {
        self.inc_by(1)
    }

    fn inc_by(&self, v: Self::Number) -> Self::Number {
        self.fetch_add(v, Ordering::Relaxed)
    }

    fn get(&self) -> Self::Number {
        self.load(Ordering::Relaxed)
    }
}

impl<A> TypedMetric for Counter<A> {
    const TYPE: MetricType = MetricType::Counter;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inc_and_get() {
        let counter: Counter = Counter::default();
        assert_eq!(0, counter.inc());
        assert_eq!(1, counter.get());
    }
}
