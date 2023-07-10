//! Metric registry implementation.
//!
//! See [`Registry`] for details.

use std::borrow::Cow;

use crate::collector::Collector;
use crate::MaybeOwned;

/// A metric registry.
///
/// First off one registers metrics with the registry via
/// [`Registry::register`]. Later on the [`Registry`] is passed to an encoder
/// collecting samples of each metric by iterating all metrics in the
/// [`Registry`].
///
/// [`Registry`] is the core building block, generic over the metric type being
/// registered. Out of convenience, the generic type parameter is set to use
/// dynamic dispatching by default to be able to register different types of
/// metrics (e.g. [`Counter`](crate::metrics::counter::Counter) and
/// [`Gauge`](crate::metrics::gauge::Gauge)) with the same registry. Advanced
/// users might want to use their custom types.
///
/// ```
/// # use prometheus_client::encoding::text::encode;
/// # use prometheus_client::metrics::counter::{Atomic as _, Counter};
/// # use prometheus_client::metrics::gauge::{Atomic as _, Gauge};
/// # use prometheus_client::registry::Registry;
/// #
/// // Create a metric registry.
/// let mut registry = Registry::default();
///
/// let counter: Counter = Counter::default();
/// let gauge: Gauge = Gauge::default();
///
/// registry.register(
///   "my_counter",
///   "This is my counter",
///   counter.clone(),
/// );
/// registry.register(
///   "my_gauge",
///   "This is my gauge",
///   gauge.clone(),
/// );
///
/// # // Encode all metrics in the registry in the text format.
/// # let mut buffer = String::new();
/// # encode(&mut buffer, &registry).unwrap();
/// #
/// # let expected = "# HELP my_counter This is my counter.\n".to_owned() +
/// #                "# TYPE my_counter counter\n" +
/// #                "my_counter_total 0\n" +
/// #                "# HELP my_gauge This is my gauge.\n" +
/// #                "# TYPE my_gauge gauge\n" +
/// #                "my_gauge 0\n" +
/// #                "# EOF\n";
/// # assert_eq!(expected, buffer);
/// ```
#[derive(Debug, Default)]
pub struct Registry {
    prefix: Option<Prefix>,
    labels: Vec<(Cow<'static, str>, Cow<'static, str>)>,
    metrics: Vec<(Descriptor, Box<dyn Metric>)>,
    collectors: Vec<Box<dyn Collector>>,
    sub_registries: Vec<Registry>,
}

impl Registry {
    /// Creates a new default [`Registry`] with the given prefix.
    pub fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            prefix: Some(Prefix(prefix.into())),
            ..Default::default()
        }
    }

    /// Creates a new default [`Registry`] with the given labels.
    pub fn with_labels(
        labels: impl Iterator<Item = (Cow<'static, str>, Cow<'static, str>)>,
    ) -> Self {
        Self {
            labels: labels.into_iter().collect(),
            ..Default::default()
        }
    }

    /// Creates a new default [`Registry`] with the given prefix and labels.
    pub fn with_prefix_and_labels(
        prefix: impl Into<String>,
        labels: impl Iterator<Item = (Cow<'static, str>, Cow<'static, str>)>,
    ) -> Self {
        Self {
            prefix: Some(Prefix(prefix.into())),
            labels: labels.into_iter().collect(),
            ..Default::default()
        }
    }

    /// Register a metric with the [`Registry`].
    ///
    /// Note: In the Open Metrics text exposition format some metric types have
    /// a special suffix, e.g. the
    /// [`Counter`](crate::metrics::counter::Counter`) metric with `_total`.
    /// These suffixes are inferred through the metric type and must not be
    /// appended to the metric name manually by the user.
    ///
    /// Note: A full stop punctuation mark (`.`) is automatically added to the
    /// passed help text.
    ///
    /// Use [`Registry::register_with_unit`] whenever a unit for the given
    /// metric is known.
    ///
    /// ```
    /// # use prometheus_client::metrics::counter::{Atomic as _, Counter};
    /// # use prometheus_client::registry::{Registry, Unit};
    /// #
    /// let mut registry = Registry::default();
    /// let counter: Counter = Counter::default();
    ///
    /// registry.register("my_counter", "This is my counter", counter.clone());
    /// ```
    pub fn register<N: Into<String>, H: Into<String>>(
        &mut self,
        name: N,
        help: H,
        metric: impl Metric,
    ) {
        self.priv_register(name, help, metric, None)
    }

    /// Register a metric with the [`Registry`] specifying the metric's unit.
    ///
    /// See [`Registry::register`] for additional documentation.
    ///
    /// Note: In the Open Metrics text exposition format units are appended to
    /// the metric name. This is done automatically. Users must not append the
    /// unit to the name manually.
    ///
    /// ```
    /// # use prometheus_client::metrics::counter::{Atomic as _, Counter};
    /// # use prometheus_client::registry::{Registry, Unit};
    /// #
    /// let mut registry = Registry::default();
    /// let counter: Counter = Counter::default();
    ///
    /// registry.register_with_unit(
    ///   "my_counter",
    ///   "This is my counter",
    ///   Unit::Seconds,
    ///   counter.clone(),
    /// );
    /// ```
    pub fn register_with_unit<N: Into<String>, H: Into<String>>(
        &mut self,
        name: N,
        help: H,
        unit: Unit,
        metric: impl Metric,
    ) {
        self.priv_register(name, help, metric, Some(unit))
    }

    fn priv_register<N: Into<String>, H: Into<String>>(
        &mut self,
        name: N,
        help: H,
        metric: impl Metric,
        unit: Option<Unit>,
    ) {
        let descriptor =
            Descriptor::new(name, help, unit, self.prefix.as_ref(), self.labels.clone());

        self.metrics.push((descriptor, Box::new(metric)));
    }

    /// Register a [`Collector`].
    ///
    /// ```
    /// # use prometheus_client::metrics::counter::ConstCounter;
    /// # use prometheus_client::registry::{Descriptor, Registry, LocalMetric};
    /// # use prometheus_client::collector::Collector;
    /// # use prometheus_client::MaybeOwned;
    /// # use std::borrow::Cow;
    /// #
    /// #[derive(Debug)]
    /// struct MyCollector {}
    ///
    /// impl Collector for MyCollector {
    ///   fn collect<'a>(&'a self) -> Box<dyn Iterator<Item = (Cow<'a, Descriptor>, MaybeOwned<'a, Box<dyn LocalMetric>>)> + 'a> {
    ///     let c: Box<dyn LocalMetric> = Box::new(ConstCounter::new(42));
    ///     let descriptor = Descriptor::new(
    ///       "my_counter",
    ///       "This is my counter",
    ///       None,
    ///       None,
    ///       vec![],
    ///     );
    ///     Box::new(std::iter::once((Cow::Owned(descriptor), MaybeOwned::Owned(c))))
    ///   }
    /// }
    ///
    /// let my_collector = Box::new(MyCollector{});
    ///
    /// let mut registry = Registry::default();
    ///
    /// registry.register_collector(my_collector);
    /// ```
    pub fn register_collector(&mut self, collector: Box<dyn Collector>) {
        self.collectors.push(collector);
    }

    /// Create a sub-registry to register metrics with a common prefix.
    ///
    /// Say you would like to prefix one set of metrics with `subsystem_a` and
    /// one set of metrics with `subsystem_b`. Instead of prefixing each metric
    /// with the corresponding subsystem string individually, you can create two
    /// sub-registries like demonstrated below.
    ///
    /// This can be used to pass a prefixed sub-registry down to a subsystem of
    /// your architecture automatically adding a prefix to each metric the
    /// subsystem registers.
    ///
    /// ```
    /// # use prometheus_client::metrics::counter::{Atomic as _, Counter};
    /// # use prometheus_client::registry::{Registry, Unit};
    /// #
    /// let mut registry = Registry::default();
    ///
    /// let subsystem_a_counter_1: Counter = Counter::default();
    /// let subsystem_a_counter_2: Counter = Counter::default();
    ///
    /// let subsystem_a_registry = registry.sub_registry_with_prefix("subsystem_a");
    /// registry.register("counter_1", "", subsystem_a_counter_1.clone());
    /// registry.register("counter_2", "", subsystem_a_counter_2.clone());
    ///
    /// let subsystem_b_counter_1: Counter = Counter::default();
    /// let subsystem_b_counter_2: Counter = Counter::default();
    ///
    /// let subsystem_a_registry = registry.sub_registry_with_prefix("subsystem_b");
    /// registry.register("counter_1", "", subsystem_b_counter_1.clone());
    /// registry.register("counter_2", "", subsystem_b_counter_2.clone());
    /// ```
    ///
    /// See [`Registry::sub_registry_with_label`] for the same functionality,
    /// but namespacing with a label instead of a metric name prefix.
    pub fn sub_registry_with_prefix<P: AsRef<str>>(&mut self, prefix: P) -> &mut Self {
        let sub_registry = Registry {
            prefix: Some(Prefix(
                self.prefix.clone().map(|p| p.0 + "_").unwrap_or_default() + prefix.as_ref(),
            )),
            labels: self.labels.clone(),
            ..Default::default()
        };

        self.priv_sub_registry(sub_registry)
    }

    /// Like [`Registry::sub_registry_with_prefix`] but with a label instead.
    pub fn sub_registry_with_label(
        &mut self,
        label: (Cow<'static, str>, Cow<'static, str>),
    ) -> &mut Self {
        self.sub_registry_with_labels(std::iter::once(label))
    }

    /// Like [`Registry::sub_registry_with_prefix`] but with multiple labels instead.
    pub fn sub_registry_with_labels(
        &mut self,
        labels: impl Iterator<Item = (Cow<'static, str>, Cow<'static, str>)>,
    ) -> &mut Self {
        let mut new_labels = self.labels.clone();
        new_labels.extend(labels);

        let sub_registry = Registry {
            prefix: self.prefix.clone(),
            labels: new_labels,
            ..Default::default()
        };

        self.priv_sub_registry(sub_registry)
    }

    fn priv_sub_registry(&mut self, sub_registry: Self) -> &mut Self {
        self.sub_registries.push(sub_registry);

        self.sub_registries
            .last_mut()
            .expect("sub_registries not to be empty.")
    }

    pub(crate) fn iter_metrics(&self) -> MetricIterator {
        let metrics = self.metrics.iter();
        let sub_registries = self.sub_registries.iter();
        MetricIterator {
            metrics,
            sub_registries,
            sub_registry: None,
        }
    }

    pub(crate) fn iter_collectors(&self) -> CollectorIterator {
        let collectors = self.collectors.iter();
        let sub_registries = self.sub_registries.iter();
        CollectorIterator {
            prefix: self.prefix.as_ref(),
            labels: &self.labels,

            collector: None,
            collectors,

            sub_collector_iter: None,
            sub_registries,
        }
    }
}

/// Iterator iterating both the metrics registered directly with the
/// [`Registry`] as well as all metrics registered with sub [`Registry`]s.
#[derive(Debug)]
pub struct MetricIterator<'a> {
    metrics: std::slice::Iter<'a, (Descriptor, Box<dyn Metric>)>,
    sub_registries: std::slice::Iter<'a, Registry>,
    sub_registry: Option<Box<MetricIterator<'a>>>,
}

impl<'a> Iterator for MetricIterator<'a> {
    type Item = &'a (Descriptor, Box<dyn Metric>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(m) = self.metrics.next() {
                return Some(m);
            }

            if let Some(metric) = self.sub_registry.as_mut().and_then(|i| i.next()) {
                return Some(metric);
            }

            self.sub_registry = self
                .sub_registries
                .next()
                .map(|r| Box::new(r.iter_metrics()));

            if self.sub_registry.is_none() {
                break;
            }
        }

        None
    }
}

/// Iterator iterating metrics retrieved from [`Collector`]s registered with the [`Registry`] or sub [`Registry`]s.
pub struct CollectorIterator<'a> {
    prefix: Option<&'a Prefix>,
    labels: &'a [(Cow<'static, str>, Cow<'static, str>)],

    #[allow(clippy::type_complexity)]
    collector: Option<
        Box<dyn Iterator<Item = (Cow<'a, Descriptor>, MaybeOwned<'a, Box<dyn LocalMetric>>)> + 'a>,
    >,
    collectors: std::slice::Iter<'a, Box<dyn Collector>>,

    sub_collector_iter: Option<Box<CollectorIterator<'a>>>,
    sub_registries: std::slice::Iter<'a, Registry>,
}

impl<'a> std::fmt::Debug for CollectorIterator<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectorIterator")
            .field("prefix", &self.prefix)
            .field("labels", &self.labels)
            .finish()
    }
}

impl<'a> Iterator for CollectorIterator<'a> {
    type Item = (Cow<'a, Descriptor>, MaybeOwned<'a, Box<dyn LocalMetric>>);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(m) = self
                .collector
                .as_mut()
                .and_then(|c| c.next())
                .or_else(|| self.sub_collector_iter.as_mut().and_then(|i| i.next()))
                .map(|(descriptor, metric)| {
                    if self.prefix.is_some() || !self.labels.is_empty() {
                        let Descriptor {
                            name,
                            help,
                            unit,
                            labels,
                        } = descriptor.as_ref();
                        let mut labels = labels.to_vec();
                        labels.extend_from_slice(self.labels);
                        let enriched_descriptor =
                            Descriptor::new(name, help, unit.to_owned(), self.prefix, labels);

                        Some((Cow::Owned(enriched_descriptor), metric))
                    } else {
                        Some((descriptor, metric))
                    }
                })
            {
                return m;
            }

            if let Some(collector) = self.collectors.next() {
                self.collector = Some(collector.collect());
                continue;
            }

            if let Some(collector_iter) = self
                .sub_registries
                .next()
                .map(|r| Box::new(r.iter_collectors()))
            {
                self.sub_collector_iter = Some(collector_iter);
                continue;
            }

            return None;
        }
    }
}

/// Metric prefix
#[derive(Clone, Debug)]
pub struct Prefix(String);

impl Prefix {
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl From<String> for Prefix {
    fn from(s: String) -> Self {
        Prefix(s)
    }
}

/// OpenMetrics metric descriptor.
#[derive(Debug, Clone)]
pub struct Descriptor {
    name: String,
    help: String,
    unit: Option<Unit>,
    labels: Vec<(Cow<'static, str>, Cow<'static, str>)>,
}

impl Descriptor {
    /// Create new [`Descriptor`].
    pub fn new<N: Into<String>, H: Into<String>>(
        name: N,
        help: H,
        unit: Option<Unit>,
        prefix: Option<&Prefix>,
        labels: Vec<(Cow<'static, str>, Cow<'static, str>)>,
    ) -> Self {
        let mut name = name.into();
        if let Some(prefix) = prefix {
            name.insert(0, '_');
            name.insert_str(0, prefix.as_str());
        }

        let help = help.into() + ".";

        Descriptor {
            name,
            help,
            unit,
            labels,
        }
    }

    /// Returns the name of the OpenMetrics metric [`Descriptor`].
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the help text of the OpenMetrics metric [`Descriptor`].
    pub fn help(&self) -> &str {
        &self.help
    }

    /// Returns the unit of the OpenMetrics metric [`Descriptor`].
    pub fn unit(&self) -> &Option<Unit> {
        &self.unit
    }

    /// Returns the label set of the OpenMetrics metric [`Descriptor`].
    pub fn labels(&self) -> &[(Cow<'static, str>, Cow<'static, str>)] {
        &self.labels
    }
}

/// Metric units recommended by Open Metrics.
///
/// See [`Unit::Other`] to specify alternative units.
#[derive(Debug, Clone)]
#[allow(missing_docs)]
pub enum Unit {
    Amperes,
    Bytes,
    Celsius,
    Grams,
    Joules,
    Meters,
    Ratios,
    Seconds,
    Volts,
    Other(String),
}

impl Unit {
    /// Returns the given Unit's str representation.
    pub fn as_str(&self) -> &str {
        match self {
            Unit::Amperes => "amperes",
            Unit::Bytes => "bytes",
            Unit::Celsius => "celsius",
            Unit::Grams => "grams",
            Unit::Joules => "joules",
            Unit::Meters => "meters",
            Unit::Ratios => "ratios",
            Unit::Seconds => "seconds",
            Unit::Volts => "volts",
            Unit::Other(other) => other.as_str(),
        }
    }
}

/// Super trait representing an abstract Prometheus metric.
pub trait Metric: crate::encoding::EncodeMetric + Send + Sync + std::fmt::Debug + 'static {}

impl<T> Metric for T where T: crate::encoding::EncodeMetric + Send + Sync + std::fmt::Debug + 'static
{}

/// Similar to [`Metric`], but without the [`Send`] and [`Sync`] requirement.
pub trait LocalMetric: crate::encoding::EncodeMetric + std::fmt::Debug {}

impl<T> LocalMetric for T where T: crate::encoding::EncodeMetric + std::fmt::Debug {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::counter::Counter;

    #[test]
    fn constructors() {
        let counter_name = "test_counter";
        let prefix = "test_prefix";
        let labels = vec![
            (Cow::Borrowed("global_label_1"), Cow::Borrowed("value_1")),
            (Cow::Borrowed("global_label_1"), Cow::Borrowed("value_2")),
        ];
        // test with_prefix constructor
        let mut registry = Registry::with_prefix(prefix);
        let counter: Counter = Counter::default();
        registry.register(counter_name, "some help", counter);

        assert_eq!(
            Some((prefix.to_string() + "_" + counter_name, vec![])),
            registry
                .iter_metrics()
                .map(|(desc, _)| (desc.name.clone(), desc.labels.clone()))
                .next()
        );

        // test with_labels constructor
        let mut registry = Registry::with_labels(labels.clone().into_iter());
        let counter: Counter = Counter::default();
        registry.register(counter_name, "some help", counter);
        assert_eq!(
            Some((counter_name.to_string(), labels.clone())),
            registry
                .iter_metrics()
                .map(|(desc, _)| (desc.name.clone(), desc.labels.clone()))
                .next()
        );

        // test with_prefix_and_labels constructor
        let mut registry = Registry::with_prefix_and_labels(prefix, labels.clone().into_iter());
        let counter: Counter = Counter::default();
        registry.register(counter_name, "some help", counter);

        assert_eq!(
            Some((prefix.to_string() + "_" + counter_name, labels)),
            registry
                .iter_metrics()
                .map(|(desc, _)| (desc.name.clone(), desc.labels.clone()))
                .next()
        );
    }

    #[test]
    fn register_and_iterate() {
        let mut registry = Registry::default();
        let counter: Counter = Counter::default();
        registry.register("my_counter", "My counter", counter);

        assert_eq!(1, registry.iter_metrics().count())
    }

    #[test]
    fn sub_registry_with_prefix_and_label() {
        let top_level_metric_name = "my_top_level_metric";
        let mut registry = Registry::default();
        let counter: Counter = Counter::default();
        registry.register(top_level_metric_name, "some help", counter.clone());

        let prefix_1 = "prefix_1";
        let prefix_1_metric_name = "my_prefix_1_metric";
        let sub_registry = registry.sub_registry_with_prefix(prefix_1);
        sub_registry.register(prefix_1_metric_name, "some help", counter.clone());

        let prefix_1_1 = "prefix_1_1";
        let prefix_1_1_metric_name = "my_prefix_1_1_metric";
        let sub_sub_registry = sub_registry.sub_registry_with_prefix(prefix_1_1);
        sub_sub_registry.register(prefix_1_1_metric_name, "some help", counter.clone());

        let label_1_2 = (Cow::Borrowed("registry"), Cow::Borrowed("1_2"));
        let prefix_1_2_metric_name = "my_prefix_1_2_metric";
        let sub_sub_registry = sub_registry.sub_registry_with_label(label_1_2.clone());
        sub_sub_registry.register(prefix_1_2_metric_name, "some help", counter.clone());

        let labels_1_3 = vec![
            (Cow::Borrowed("label_1_3_1"), Cow::Borrowed("value_1_3_1")),
            (Cow::Borrowed("label_1_3_2"), Cow::Borrowed("value_1_3_2")),
        ];
        let prefix_1_3_metric_name = "my_prefix_1_3_metric";
        let sub_sub_registry =
            sub_registry.sub_registry_with_labels(labels_1_3.clone().into_iter());
        sub_sub_registry.register(prefix_1_3_metric_name, "some help", counter.clone());

        let prefix_1_3_1 = "prefix_1_3_1";
        let prefix_1_3_1_metric_name = "my_prefix_1_3_1_metric";
        let sub_sub_sub_registry = sub_sub_registry.sub_registry_with_prefix(prefix_1_3_1);
        sub_sub_sub_registry.register(prefix_1_3_1_metric_name, "some help", counter.clone());

        let prefix_2 = "prefix_2";
        let _ = registry.sub_registry_with_prefix(prefix_2);

        let prefix_3 = "prefix_3";
        let prefix_3_metric_name = "my_prefix_3_metric";
        let sub_registry = registry.sub_registry_with_prefix(prefix_3);
        sub_registry.register(prefix_3_metric_name, "some help", counter);

        let mut metric_iter = registry
            .iter_metrics()
            .map(|(desc, _)| (desc.name.clone(), desc.labels.clone()));
        assert_eq!(
            Some((top_level_metric_name.to_string(), vec![])),
            metric_iter.next()
        );
        assert_eq!(
            Some((prefix_1.to_string() + "_" + prefix_1_metric_name, vec![])),
            metric_iter.next()
        );
        assert_eq!(
            Some((
                prefix_1.to_string() + "_" + prefix_1_1 + "_" + prefix_1_1_metric_name,
                vec![]
            )),
            metric_iter.next()
        );
        assert_eq!(
            Some((
                prefix_1.to_string() + "_" + prefix_1_2_metric_name,
                vec![label_1_2.clone()]
            )),
            metric_iter.next()
        );
        assert_eq!(
            Some((
                prefix_1.to_string() + "_" + prefix_1_3_metric_name,
                labels_1_3.clone()
            )),
            metric_iter.next()
        );
        assert_eq!(
            Some((
                prefix_1.to_string() + "_" + prefix_1_3_1 + "_" + prefix_1_3_1_metric_name,
                labels_1_3.clone()
            )),
            metric_iter.next()
        );
        // No metric was registered with prefix 2.
        assert_eq!(
            Some((prefix_3.to_string() + "_" + prefix_3_metric_name, vec![])),
            metric_iter.next()
        );
    }
}
