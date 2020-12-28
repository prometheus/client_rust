// use crate::counter::Counter;
// use crate::gauge::Gauge;
// use crate::Histogram;
// use crate::family::MetricFamily;

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

// // TODO: In the ideal case one could use dynamic dispatching to pass different
// // metric types wrapped in a box to a single registry. Problem is that
// // `EncodeMetric` cannot be made into an object as its `encode` method has
// // generic parameters.
// //
// // This is a hack to solve the above. An alternative solution would be very much
// // appreciated.
// enum Metric {
//     Counter(Counter),
//     Gauge(Gauge),
//     Histogram(Histogram),
//     MetricFamily(MetricFamily),
// }
//
// // TODO: This is a hack. See `Metric`.
// impl Registry<Metric> {
//     fn register_counter<>(&mut self, name: String, help: String, counter: Counter<>) {
//
//     }
// }

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
