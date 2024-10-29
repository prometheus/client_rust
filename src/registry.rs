//! Metric registry implementation.
//!
//! See [`Registry`] for details.

use std::borrow::Cow;

use crate::collector::Collector;
use crate::encoding::{DescriptorEncoder, EncodeMetric};

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
        let descriptor = Descriptor::new(name, help, unit);
        self.metrics.push((descriptor, Box::new(metric)));
    }

    /// Register a [`Collector`].
    ///
    /// ```
    /// # use prometheus_client::metrics::counter::ConstCounter;
    /// # use prometheus_client::registry::Registry;
    /// # use prometheus_client::collector::Collector;
    /// # use prometheus_client::encoding::{DescriptorEncoder, EncodeMetric};
    /// #
    /// #[derive(Debug)]
    /// struct MyCollector {}
    ///
    /// impl Collector for MyCollector {
    ///     fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
    ///         let counter = ConstCounter::new(42u64);
    ///         let metric_encoder = encoder.encode_descriptor(
    ///             "my_counter",
    ///             "some help",
    ///             None,
    ///             counter.metric_type(),
    ///         )?;
    ///         counter.encode(metric_encoder)?;
    ///         Ok(())
    ///     }
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

    pub(crate) fn encode(&self, encoder: &mut DescriptorEncoder) -> Result<(), std::fmt::Error> {
        for (descriptor, metric) in self.metrics.iter() {
            let mut descriptor_encoder =
                encoder.with_prefix_and_labels(self.prefix.as_ref(), &self.labels);
            let metric_encoder = descriptor_encoder.encode_descriptor(
                &descriptor.name,
                &descriptor.help,
                descriptor.unit.as_ref(),
                EncodeMetric::metric_type(metric.as_ref()),
            )?;
            metric.encode(metric_encoder)?;
        }

        for collector in self.collectors.iter() {
            let descriptor_encoder =
                encoder.with_prefix_and_labels(self.prefix.as_ref(), &self.labels);
            collector.encode(descriptor_encoder)?;
        }

        for registry in self.sub_registries.iter() {
            registry.encode(encoder)?;
        }

        Ok(())
    }
}

/// Metric prefix
#[derive(Clone, Debug)]
pub(crate) struct Prefix(String);

impl Prefix {
    pub(crate) fn as_str(&self) -> &str {
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
struct Descriptor {
    name: String,
    help: String,
    unit: Option<Unit>,
}

impl Descriptor {
    /// Create new [`Descriptor`].
    fn new<N: Into<String>, H: Into<String>>(name: N, help: H, unit: Option<Unit>) -> Self {
        Self {
            name: name.into(),
            help: help.into() + ".",
            unit,
        }
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
