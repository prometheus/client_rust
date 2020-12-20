use std::io::Write;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

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

    pub fn get(&self) -> A::Number {
        self.value.get()
    }

    // TODO: For advanced use-cases, how about an `fn inner`?
}

pub trait Atomic {
    type Number;

    fn new() -> Self;

    fn inc(&self) -> Self::Number;

    fn get(&self) -> Self::Number;
}

impl<A> Default for Counter<A> where A: Default {
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
        self.fetch_add(1, Ordering::Relaxed)
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
        self.fetch_add(1, Ordering::Relaxed)
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
        let counter = Counter::<AtomicU64>::new();
        assert_eq!(0, counter.inc());
        assert_eq!(1, counter.get());
    }
}
