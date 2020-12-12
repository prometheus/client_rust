pub struct Registry<M> {
    metrics: Vec<M>,
}

impl<M> Registry<M> {
    pub fn new() -> Self {
        Self {
            metrics: Default::default(),
        }
    }

    pub fn register(&mut self, metric: M) {
        self.metrics.push(metric);
    }

    pub fn iter(&self) -> impl Iterator<Item = &M> {
        self.metrics.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counter::Counter;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn register_and_iterate() {
        let mut registry = Registry::new();
        let counter = Counter::<AtomicU64>::new();
        registry.register(counter.clone());

        assert_eq!(1, registry.iter().count())
    }
}
