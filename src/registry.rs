//! Metric registry implementation.
//!
//! See [`Registry`] for details.

use std::borrow::Cow;
use std::ops::Add;

/// A metric registry.
///
/// First off one registers metrics with the registry via
/// [`Registry::register`]. Later on the [`Registry`] is passed to an encoder
/// collecting samples of each metric by iterating all metrics in the
/// [`Registry`] via [`Registry::iter`].
///
/// [`Registry`] is the core building block, generic over the metric type being
/// registered. Out of convenience, the generic type parameter is set to use
/// dynamic dispatching by default to be able to register different types of
/// metrics (e.g. [`Counter`](crate::metrics::counter::Counter) and
/// [`Gauge`](crate::metrics::gauge::Gauge)) with the same registry. Advanced
/// users might want to use their custom types.
///
/// ```
/// # use prometheus_client::encoding::text::{encode, EncodeMetric};
/// # use prometheus_client::metrics::counter::{Atomic as _, Counter};
/// # use prometheus_client::metrics::gauge::{Atomic as _, Gauge};
/// # use prometheus_client::registry::Registry;
/// #
/// // Create a metric registry.
/// //
/// // Note the angle brackets to make sure to use the default (dynamic
/// // dispatched boxed metric) for the generic type parameter.
/// let mut registry = <Registry>::default();
///
/// let counter: Counter = Counter::default();
/// let gauge: Gauge = Gauge::default();
///
/// registry.register(
///   "my_counter",
///   "This is my counter",
///   Box::new(counter.clone()),
/// );
/// registry.register(
///   "my_gauge",
///   "This is my gauge",
///   Box::new(gauge.clone()),
/// );
///
/// # // Encode all metrics in the registry in the text format.
/// # let mut buffer = vec![];
/// # encode(&mut buffer, &registry).unwrap();
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
pub struct Registry<M = Box<dyn crate::encoding::text::SendSyncEncodeMetric>> {
    prefix: Option<Prefix>,
    labels: Vec<(Cow<'static, str>, Cow<'static, str>)>,
    metrics: Vec<(Descriptor, M)>,
    sub_registries: Vec<Registry<M>>,
}

impl<M> Default for Registry<M> {
    fn default() -> Self {
        Self {
            prefix: None,
            labels: Default::default(),
            metrics: Default::default(),
            sub_registries: vec![],
        }
    }
}

impl<M> Registry<M> {
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
    /// let mut registry: Registry<Counter> = Registry::default();
    /// let counter = Counter::default();
    ///
    /// registry.register("my_counter", "This is my counter", counter.clone());
    /// ```
    pub fn register<N: Into<String>, H: Into<String>>(&mut self, name: N, help: H, metric: M) {
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
    /// let mut registry: Registry<Counter> = Registry::default();
    /// let counter = Counter::default();
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
        metric: M,
    ) {
        self.priv_register(name, help, metric, Some(unit))
    }

    fn priv_register<N: Into<String>, H: Into<String>>(
        &mut self,
        name: N,
        help: H,
        metric: M,
        unit: Option<Unit>,
    ) {
        let name = name.into();
        let help = help.into() + ".";
        let descriptor = Descriptor {
            name: self
                .prefix
                .as_ref()
                .map(|p| (p.clone() + "_" + name.as_str()).into())
                .unwrap_or(name),
            help,
            unit,
            labels: self.labels.clone(),
        };

        self.metrics.push((descriptor, metric));
    }

    // TODO: Update doc.
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
    /// let mut registry: Registry<Counter> = Registry::default();
    ///
    /// let subsystem_a_counter_1 = Counter::default();
    /// let subsystem_a_counter_2 = Counter::default();
    ///
    /// let subsystem_a_registry = registry.sub_registry_with_prefix("subsystem_a");
    /// registry.register("counter_1", "", subsystem_a_counter_1.clone());
    /// registry.register("counter_2", "", subsystem_a_counter_2.clone());
    ///
    /// let subsystem_b_counter_1 = Counter::default();
    /// let subsystem_b_counter_2 = Counter::default();
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
            prefix: Some(
                self.prefix
                    .clone()
                    .map(|p| p + "_")
                    .unwrap_or_else(|| String::new().into())
                    + prefix.as_ref(),
            ),
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
        let mut labels = self.labels.clone();
        labels.push(label);
        let sub_registry = Registry {
            prefix: self.prefix.clone(),
            labels,
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
    name: String,
    help: String,
    unit: Option<Unit>,
    labels: Vec<(Cow<'static, str>, Cow<'static, str>)>,
}

impl Descriptor {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn help(&self) -> &str {
        &self.help
    }

    pub fn unit(&self) -> &Option<Unit> {
        &self.unit
    }

    pub fn labels(&self) -> &[(Cow<'static, str>, Cow<'static, str>)] {
        &self.labels
    }
}

/// Metric units recommended by Open Metrics.
///
/// See [`Unit::Other`] to specify alternative units.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::counter::Counter;

    #[test]
    fn register_and_iterate() {
        let mut registry: Registry<Counter> = Registry::default();
        let counter = Counter::default();
        registry.register("my_counter", "My counter", counter.clone());

        assert_eq!(1, registry.iter().count())
    }

    #[test]
    fn sub_registry_with_prefix_and_label() {
        let top_level_metric_name = "my_top_level_metric";
        let mut registry = Registry::<Counter>::default();
        registry.register(top_level_metric_name, "some help", Default::default());

        let prefix_1 = "prefix_1";
        let prefix_1_metric_name = "my_prefix_1_metric";
        let sub_registry = registry.sub_registry_with_prefix(prefix_1);
        sub_registry.register(prefix_1_metric_name, "some help", Default::default());

        let prefix_1_1 = "prefix_1_1";
        let prefix_1_1_metric_name = "my_prefix_1_1_metric";
        let sub_sub_registry = sub_registry.sub_registry_with_prefix(prefix_1_1);
        sub_sub_registry.register(prefix_1_1_metric_name, "some help", Default::default());

        let label_1_2 = (Cow::Borrowed("registry"), Cow::Borrowed("1_2"));
        let prefix_1_2_metric_name = "my_prefix_1_2_metric";
        let sub_sub_registry = sub_registry.sub_registry_with_label(label_1_2.clone());
        sub_sub_registry.register(prefix_1_2_metric_name, "some help", Default::default());

        let prefix_1_2_1 = "prefix_1_2_1";
        let prefix_1_2_1_metric_name = "my_prefix_1_2_1_metric";
        let sub_sub_sub_registry = sub_sub_registry.sub_registry_with_prefix(prefix_1_2_1);
        sub_sub_sub_registry.register(prefix_1_2_1_metric_name, "some help", Default::default());

        let prefix_2 = "prefix_2";
        let _ = registry.sub_registry_with_prefix(prefix_2);

        let prefix_3 = "prefix_3";
        let prefix_3_metric_name = "my_prefix_3_metric";
        let sub_registry = registry.sub_registry_with_prefix(prefix_3);
        sub_registry.register(prefix_3_metric_name, "some help", Default::default());

        let mut metric_iter = registry
            .iter()
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
                prefix_1.to_string() + "_" + prefix_1_2_1 + "_" + prefix_1_2_1_metric_name,
                vec![label_1_2]
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
