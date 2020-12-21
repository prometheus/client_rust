use owning_ref::OwningRef;
use std::sync::{Arc, Mutex, MutexGuard};

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
    sum: f64,
    count: u64,
    // TODO: Consider being generic over the bucket length.
    buckets: Vec<(f64, u64)>,
}

impl Histogram {
    pub fn new(buckets: Vec<f64>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                sum: Default::default(),
                count: Default::default(),
                buckets: buckets
                    .into_iter()
                    .map(|upper_bound| (upper_bound, 0))
                    .collect(),
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

    pub(crate) fn sum(&self) -> f64 {
        self.inner.lock().unwrap().sum
    }

    pub(crate) fn count(&self) -> u64 {
        self.inner.lock().unwrap().count
    }

    pub(crate) fn buckets(&self) -> OwningRef<MutexGuard<Inner>, Vec<(f64, u64)>> {
        let inner = self.inner.lock().unwrap();
        OwningRef::new(inner).map(|inner| &inner.buckets)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram() {
        let histogram = Histogram::new(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
        histogram.observe(1.0);
    }
}
