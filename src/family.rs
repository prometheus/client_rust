use crate::counter::Counter;
use crate::label::LabelSet;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

pub struct MetricFamily<S, M> {
    metrics: HashMap<S, M>,
}

impl<S: LabelSet, M> MetricFamily<S, M> {
    fn new() -> Self {
        Self {
            metrics: Default::default(),
        }
    } 

    fn entry(&mut self, s: S) -> Entry<S, M> {
        self.metrics.entry(s)
    }
}

#[cfg(test)]
mod tests {
    use crate::counter::{Atomic, Counter};
    use std::sync::atomic::AtomicU64;
    use super::*;

    fn counter_family() {
        let mut family = MetricFamily::<Vec<(String, String)>, Counter<AtomicU64>>::new();

        family.entry(vec![("method".to_string(), "GET".to_string())])
            .or_default()
            .inc();
    }
}
