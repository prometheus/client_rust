pub struct Registry<M> {
    metrics: Vec<(Descriptor, M)>,
}

impl<M> Registry<M> {
    pub fn new() -> Self {
        Self {
            metrics: Default::default(),
        }
    }

    pub fn register(&mut self, desc: Descriptor, metric: M) {
        self.metrics.push((desc, metric));
    }

    pub fn iter(&self) -> impl Iterator<Item = &(Descriptor, M)> {
        self.metrics.iter()
    }
}

pub struct Descriptor {
    m_type: String,
    help: String,
    name: String,
}

impl Descriptor {
    pub fn new<T: ToString, H: ToString, N: ToString>(m_type: T, help: H, name: N) -> Self {
        Self {
            m_type: m_type.to_string(),
            help: help.to_string(),
            name: name.to_string(),
        }
    }

    pub fn m_type(&self) -> &str {
        &self.m_type
    }

    pub fn help(&self) -> &str {
        &self.help
    }

    pub fn name(&self) -> &str {
        &self.name
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
        registry.register(
            Descriptor::new("counter", "My counter", "my_counter"),
            counter.clone(),
        );

        assert_eq!(1, registry.iter().count())
    }
}
