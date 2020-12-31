use std::ops::{Add, Deref};

/// A metric registry to register metrics with, later on passed to an encoder
/// collecting samples of each metric by iterating all metrics in the registry.
///
/// ```
/// # use std::sync::atomic::AtomicU64;
/// # use open_metrics_client::counter::{Atomic as _, Counter};
/// # use open_metrics_client::gauge::{Atomic as _, Gauge};
/// # use open_metrics_client::encoding::text::{encode, EncodeMetric};
/// # use open_metrics_client::registry::{Descriptor, Registry};
/// #
/// let mut registry = Registry::<Box<dyn EncodeMetric>>::new();
/// let counter = Counter::<AtomicU64>::new();
/// let gauge= Gauge::<AtomicU64>::new();
/// registry.register(
///   Descriptor::new("counter", "This is my counter.", "my_counter"),
///   Box::new(counter.clone()),
/// );
/// registry.register(
///   Descriptor::new("gauge", "This is my gauge.", "my_gauge"),
///   Box::new(gauge.clone()),
/// );
///
/// # // Encode all metrics in the registry in the text format.
/// # let mut buffer = vec![];
/// # encode::<_, _>(&mut buffer, &registry).unwrap();
/// #
/// # let expected = "# HELP my_counter This is my counter.\n".to_owned() +
/// #                "# TYPE my_counter counter\n" +
/// #                "my_counter_total 0\n" +
/// #                "# HELP my_gauge This is my gauge.\n" +
/// #                "# TYPE my_gauge gauge\n" +
/// #                "my_gauge 0\n" +
/// #                "# EOF\n";
/// # assert_eq!(expected, String::from_utf8(buffer).unwrap());
/// ```
pub struct Registry<M> {
    prefix: Option<Prefix>,
    metrics: Vec<(Descriptor, M)>,
    sub_registries: Vec<Registry<M>>,
}

impl<M> Registry<M> {
    pub fn new() -> Self {
        Self {
            prefix: None,
            metrics: Default::default(),
            sub_registries: vec![],
        }
    }

    pub fn sub_registry(&mut self, prefix: &str) -> &mut Self {
        let prefix = self
            .prefix
            .clone()
            .map(|p| p + "_")
            .unwrap_or_else(|| String::new().into())
            + prefix;
        let mut sub_registry = Registry::new();
        sub_registry.prefix = Some(prefix);
        self.sub_registries.push(sub_registry);

        self.sub_registries
            .last_mut()
            .expect("sub_registries not to be empty.")
    }

    pub fn register(&mut self, mut desc: Descriptor, metric: M) {
        if let Some(prefix) = &self.prefix {
            desc.name = (prefix.clone() + "_" + desc.name.as_str()).into();
        }

        self.metrics.push((desc, metric));
    }

    pub fn iter(&self) -> RegistryIterator<M> {
        let metrics = self.metrics.iter();
        let sub_registries = self.sub_registries.iter();
        RegistryIterator {
            metrics,
            sub_registries,
            sub_registry: None,
        }
    }
}

/// Iterator iterating both the metrics registered directly with the registry as
/// well as all metrics registered with sub-registries.
pub struct RegistryIterator<'a, M> {
    metrics: std::slice::Iter<'a, (Descriptor, M)>,
    sub_registries: std::slice::Iter<'a, Registry<M>>,
    sub_registry: Option<Box<RegistryIterator<'a, M>>>,
}

impl<'a, M> Iterator for RegistryIterator<'a, M> {
    type Item = &'a (Descriptor, M);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(metric) = self.metrics.next() {
            return Some(metric);
        }

        loop {
            if let Some(metric) = self.sub_registry.as_mut().and_then(|i| i.next()) {
                return Some(metric);
            }

            self.sub_registry = self.sub_registries.next().map(|r| Box::new(r.iter()));

            if self.sub_registry.is_none() {
                break;
            }
        }

        None
    }
}

#[derive(Clone)]
struct Prefix(String);

impl From<String> for Prefix {
    fn from(s: String) -> Self {
        Prefix(s)
    }
}

impl From<Prefix> for String {
    fn from(p: Prefix) -> Self {
        p.0
    }
}

impl Add<&str> for Prefix {
    type Output = Self;
    fn add(self, rhs: &str) -> Self::Output {
        Prefix(self.0 + rhs)
    }
}

impl Add<&Prefix> for String {
    type Output = Self;
    fn add(self, rhs: &Prefix) -> Self::Output {
        self + rhs.0.as_str()
    }
}

pub struct Descriptor {
    // TODO: How about making type an enum.
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

pub trait SendEncodeMetric: crate::encoding::text::EncodeMetric + Send {}

impl<T: Send + crate::encoding::text::EncodeMetric> SendEncodeMetric for T {}

impl crate::encoding::text::EncodeMetric for Box<dyn SendEncodeMetric> {
    fn encode<'a, 'b>(
        &self,
        encoder: crate::encoding::text::Encoder<'a, 'b>,
    ) -> Result<(), std::io::Error> {
        self.deref().encode(encoder)
    }
}

pub type DynSendRegistry = Registry<Box<dyn SendEncodeMetric>>;

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

    #[test]
    fn sub_registry() {
        let top_level_metric_name = "my_top_level_metric";
        let mut registry = Registry::<()>::new();
        registry.register(
            Descriptor::new("unknown", "some help", top_level_metric_name),
            (),
        );

        let prefix_1 = "prefix_1";
        let prefix_1_metric_name = "my_prefix_1_metric";
        let sub_registry = registry.sub_registry(prefix_1);
        sub_registry.register(
            Descriptor::new("unknown", "some help", prefix_1_metric_name),
            (),
        );

        let prefix_1_1 = "prefix_1_1";
        let prefix_1_1_metric_name = "my_prefix_1_1_metric";
        let sub_sub_registry = sub_registry.sub_registry(prefix_1_1);
        sub_sub_registry.register(
            Descriptor::new("unknown", "some help", prefix_1_1_metric_name),
            (),
        );

        let prefix_2 = "prefix_2";
        let _ = registry.sub_registry(prefix_2);

        let prefix_3 = "prefix_3";
        let prefix_3_metric_name = "my_prefix_3_metric";
        let sub_registry = registry.sub_registry(prefix_3);
        sub_registry.register(
            Descriptor::new("unknown", "some help", prefix_3_metric_name),
            (),
        );

        let mut metric_iter = registry.iter().map(|(desc, _)| desc.name.clone());
        assert_eq!(Some(top_level_metric_name.to_string()), metric_iter.next());
        assert_eq!(
            Some(prefix_1.to_string() + "_" + prefix_1_metric_name),
            metric_iter.next()
        );
        assert_eq!(
            Some(prefix_1.to_string() + "_" + prefix_1_1 + "_" + prefix_1_1_metric_name),
            metric_iter.next()
        );
        // No metric was registered with prefix 2.
        assert_eq!(
            Some(prefix_3.to_string() + "_" + prefix_3_metric_name),
            metric_iter.next()
        );
    }
}
