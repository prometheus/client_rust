use crate::counter::Counter;
use crate::label::LabelSet;
use std::collections::hash_map::{self, Entry};
use std::collections::HashMap;

pub struct MetricFamily<S: std::hash::Hash + Eq, M> {
    metrics: HashMap<S, M>,
}

impl<S: LabelSet + std::hash::Hash + Eq, M> MetricFamily<S, M> {
    pub fn new() -> Self {
        Self {
            metrics: Default::default(),
        }
    }

    pub fn entry(&mut self, s: S) -> Entry<S, M> {
        self.metrics.entry(s)
    }

    pub fn iter(&self) -> hash_map::Iter<S, M>{
        self.metrics.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counter::Counter;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn counter_family() {
        let mut family = MetricFamily::<Vec<(String, String)>, Counter<AtomicU64>>::new();

        family
            .entry(vec![("method".to_string(), "GET".to_string())])
            .or_default()
            .inc();

        assert_eq!(
            1,
            family
                .entry(vec![("method".to_string(), "GET".to_string())])
                .or_default()
                .get()
        );
    }
}
