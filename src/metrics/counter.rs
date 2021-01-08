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
/// ```
/// # use open_metrics_client::metrics::counter::Counter;
/// # use std::sync::atomic::AtomicU64;
/// let counter = Counter::<AtomicU64>::new();
/// counter.inc();
/// ```
pub struct Counter<A> {
    value: Arc<A>,
}

impl<A> Clone for Counter<A> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

impl<A: Atomic> Counter<A> {
    pub fn new() -> Self {
        Counter {
            value: Arc::new(A::new()),
        }
    }

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

    fn new() -> Self;

    fn inc(&self) -> Self::Number;

    fn inc_by(&self, v: Self::Number) -> Self::Number;

    fn get(&self) -> Self::Number;
}

impl<A> Default for Counter<A>
where
    A: Default,
{
    fn default() -> Self {
        Self {
            value: Arc::new(A::default()),
        }
    }
}

impl Atomic for AtomicU64 {
    type Number = u64;

    fn new() -> Self {
        AtomicU64::new(0)
    }

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

    fn new() -> Self {
        AtomicU32::new(0)
    }

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
        let counter = Counter::<AtomicU64>::new();
        assert_eq!(0, counter.inc());
        assert_eq!(1, counter.get());
    }
}
