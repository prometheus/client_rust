
use crate::label::LabelSet;
use owning_ref::OwningRef;

use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard};

pub struct MetricFamily<S, M> {
    metrics: Arc<RwLock<HashMap<S, M>>>,
}

impl<S: Clone + LabelSet + std::hash::Hash + Eq, M: Default> MetricFamily<S, M> {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Default::default())),
        }
    }

    pub fn get_or_create(&self, sample_set: &S) -> OwningRef<RwLockReadGuard<HashMap<S, M>>, M> {
        let read_guard = self.metrics.read().unwrap();
        if let Ok(metric) =
            OwningRef::new(read_guard).try_map(|metrics| metrics.get(sample_set).ok_or(()))
        {
            return metric;
        }

        let mut write_guard = self.metrics.write().unwrap();
        write_guard.insert(sample_set.clone(), Default::default());

        drop(write_guard);

        let read_guard = self.metrics.read().unwrap();
        return OwningRef::new(read_guard).map(|metrics| metrics.get(sample_set).unwrap());
    }

    pub(crate) fn read(&self) -> RwLockReadGuard<HashMap<S, M>> {
        self.metrics.read().unwrap()
    }
}

impl<S, M> Clone for MetricFamily<S, M> {
    fn clone(&self) -> Self {
        MetricFamily {
            metrics: self.metrics.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counter::Counter;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn counter_family() {
        let family = MetricFamily::<Vec<(String, String)>, Counter<AtomicU64>>::new();

        family
            .get_or_create(&vec![("method".to_string(), "GET".to_string())])
            .inc();

        assert_eq!(
            1,
            family
                .get_or_create(&vec![("method".to_string(), "GET".to_string())])
                .get()
        );
    }
}
