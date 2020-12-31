//! Module implementing an Open Metrics gauge.
//!
//! See [`Gauge`] for details.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Open Metrics [`Gauge`] to record current measurements.
///
/// Single increasing, decreasing or constant value metric.
///
/// ```
/// # use open_metrics_client::gauge::Gauge;
/// # use std::sync::atomic::AtomicU64;
/// let gauge = Gauge::<AtomicU64>::new();
/// gauge.inc();
/// ```
pub struct Gauge<A> {
    value: Arc<A>,
}

impl<A> Clone for Gauge<A> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

impl<A: Atomic> Gauge<A> {
    pub fn new() -> Self {
        Gauge {
            value: Arc::new(A::new()),
        }
    }

    pub fn inc(&self) -> A::Number {
        self.value.inc()
    }

    pub fn inc_by(&self, v: A::Number) -> A::Number {
        self.value.inc_by(v)
    }

    pub fn set(&self, v: A::Number) -> A::Number {
        self.value.set(v)
    }

    pub fn get(&self) -> A::Number {
        self.value.get()
    }
}

pub trait Atomic {
    type Number;

    fn new() -> Self;

    fn inc(&self) -> Self::Number;

    fn inc_by(&self, v: Self::Number) -> Self::Number;

    fn set(&self, v: Self::Number) -> Self::Number;

    fn get(&self) -> Self::Number;
}

impl<A> Default for Gauge<A>
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

    fn set(&self, v: Self::Number) -> Self::Number {
        self.swap(v, Ordering::Relaxed)
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

    fn set(&self, v: Self::Number) -> Self::Number {
        self.swap(v, Ordering::Relaxed)
    }

    fn get(&self) -> Self::Number {
        self.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inc_and_get() {
        let gauge = Gauge::<AtomicU64>::new();
        assert_eq!(0, gauge.inc());
        assert_eq!(1, gauge.get());
        assert_eq!(1, gauge.set(10));
        assert_eq!(10, gauge.get());
    }
}
