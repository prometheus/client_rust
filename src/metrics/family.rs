//! Module implementing an Open Metrics metric family.
//!
//! See [`Family`] for details.

use crate::encoding::{EncodeLabelSet, EncodeMetric, MetricEncoder};

use super::{MetricType, TypedMetric};
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::collections::HashMap;
use std::sync::Arc;

/// Representation of the OpenMetrics *MetricFamily* data type.
///
/// A [`Family`] is a set of metrics with the same name, help text and
/// type, differentiated by their label values thus spanning a multidimensional
/// space.
///
/// # Generic over the label set
///
/// A [`Family`] is generic over the label type. For convenience one might
/// choose a `Vec<(String, String)>`, for performance and/or type safety one might
/// define a custom type.
///
/// ## Examples
///
/// ### [`Family`] with `Vec<(String, String)>` for convenience
///
/// ```
/// # use prometheus_client::encoding::text::encode;
/// # use prometheus_client::metrics::counter::{Atomic, Counter};
/// # use prometheus_client::metrics::family::Family;
/// # use prometheus_client::registry::Registry;
/// #
/// # let mut registry = Registry::default();
/// let family = Family::<Vec<(String, String)>, Counter>::default();
/// # registry.register(
/// #   "my_counter",
/// #   "This is my counter",
/// #   family.clone(),
/// # );
///
/// // Record a single HTTP GET request.
/// family.get_or_create(&vec![("method".to_owned(), "GET".to_owned())]).inc();
///
/// # // Encode all metrics in the registry in the text format.
/// # let mut buffer = String::new();
/// # encode(&mut buffer, &registry).unwrap();
/// #
/// # let expected = "# HELP my_counter This is my counter.\n".to_owned() +
/// #                "# TYPE my_counter counter\n" +
/// #                "my_counter_total{method=\"GET\"} 1\n" +
/// #                "# EOF\n";
/// # assert_eq!(expected, buffer);
/// ```
///
/// ### [`Family`] with custom type for performance and/or type safety
///
/// Using `EncodeLabelSet` and `EncodeLabelValue` derive macro to generate
/// [`EncodeLabelSet`] for `struct`s and
/// [`EncodeLabelValue`](crate::encoding::EncodeLabelValue) for `enum`s.
///
/// ```
/// # use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue};
/// # use prometheus_client::encoding::text::encode;
/// # use prometheus_client::metrics::counter::{Atomic, Counter};
/// # use prometheus_client::metrics::family::Family;
/// # use prometheus_client::registry::Registry;
/// # use std::io::Write;
/// #
/// # let mut registry = Registry::default();
/// #[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
/// struct Labels {
///   method: Method,
/// };
///
/// #[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
/// enum Method {
///   GET,
///   PUT,
/// };
///
/// let family = Family::<Labels, Counter>::default();
/// # registry.register(
/// #   "my_counter",
/// #   "This is my counter",
/// #   family.clone(),
/// # );
///
/// // Record a single HTTP GET request.
/// family.get_or_create(&Labels { method: Method::GET }).inc();
/// #
/// # // Encode all metrics in the registry in the text format.
/// # let mut buffer = String::new();
/// # encode(&mut buffer, &registry).unwrap();
/// #
/// # let expected = "# HELP my_counter This is my counter.\n".to_owned() +
/// #                "# TYPE my_counter counter\n" +
/// #                "my_counter_total{method=\"GET\"} 1\n" +
/// #                "# EOF\n";
/// # assert_eq!(expected, buffer);
/// ```
// TODO: Consider exposing hash algorithm.
pub struct Family<S, M, C = fn() -> M> {
    metrics: Arc<RwLock<HashMap<S, M>>>,
    /// Function that when called constructs a new metric.
    ///
    /// For most metric types this would simply be its [`Default`]
    /// implementation set through [`Family::default`]. For metric types that
    /// need custom construction logic like
    /// [`Histogram`](crate::metrics::histogram::Histogram) in order to set
    /// specific buckets, a custom constructor is set via
    /// [`Family::new_with_constructor`].
    constructor: C,
}

impl<S: std::fmt::Debug, M: std::fmt::Debug, C> std::fmt::Debug for Family<S, M, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Family")
            .field("metrics", &self.metrics)
            .finish()
    }
}

/// A constructor for creating new metrics in a [`Family`] when calling
/// [`Family::get_or_create`]. Such constructor is provided via
/// [`Family::new_with_constructor`].
///
/// This is mostly used when creating histograms using constructors that need to
/// capture variables.
///
/// ```
/// # use prometheus_client::metrics::family::{Family, MetricConstructor};
/// # use prometheus_client::metrics::histogram::Histogram;
/// struct CustomBuilder {
///     buckets: Vec<f64>,
/// }
///
/// impl MetricConstructor<Histogram> for CustomBuilder {
///     fn new_metric(&self) -> Histogram {
///         // When a new histogram is created, this function will be called.
///         Histogram::new(self.buckets.iter().cloned())
///     }
/// }
///
/// let custom_builder = CustomBuilder { buckets: vec![0.0, 10.0, 100.0] };
/// let metric = Family::<(), Histogram, CustomBuilder>::new_with_constructor(custom_builder);
/// ```
pub trait MetricConstructor<M> {
    /// Create a new instance of the metric type.
    fn new_metric(&self) -> M;
}

/// In cases in which the explicit type of the metric is not required, it is
/// posible to directly provide a closure even if it captures variables.
///
/// ```
/// # use prometheus_client::metrics::family::{Family};
/// # use prometheus_client::metrics::histogram::Histogram;
/// let custom_buckets = [0.0, 10.0, 100.0];
/// let metric = Family::<(), Histogram, _>::new_with_constructor(|| {
///     Histogram::new(custom_buckets.into_iter())
/// });
/// # metric.get_or_create(&());
/// ```
impl<M, F: Fn() -> M> MetricConstructor<M> for F {
    fn new_metric(&self) -> M {
        self()
    }
}

impl<S: Clone + std::hash::Hash + Eq, M: Default> Default for Family<S, M> {
    fn default() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Default::default())),
            constructor: M::default,
        }
    }
}

impl<S: Clone + std::hash::Hash + Eq, M, C> Family<S, M, C> {
    /// Create a metric family using a custom constructor to construct new
    /// metrics.
    ///
    /// When calling [`Family::get_or_create`] a [`Family`] needs to be able to
    /// construct a new metric in case none exists for the given label set. In
    /// most cases, e.g. for [`Counter`](crate::metrics::counter::Counter)
    /// [`Family`] can just use the [`Default::default`] implementation for the
    /// metric type. For metric types such as
    /// [`Histogram`](crate::metrics::histogram::Histogram) one might want
    /// [`Family`] to construct a
    /// [`Histogram`](crate::metrics::histogram::Histogram) with custom buckets
    /// (see example below). For such case one can use this method. For more
    /// involved constructors see [`MetricConstructor`].
    ///
    /// ```
    /// # use prometheus_client::metrics::family::Family;
    /// # use prometheus_client::metrics::histogram::{exponential_buckets, Histogram};
    /// Family::<Vec<(String, String)>, Histogram>::new_with_constructor(|| {
    ///     Histogram::new(exponential_buckets(1.0, 2.0, 10))
    /// });
    /// ```
    pub fn new_with_constructor(constructor: C) -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Default::default())),
            constructor,
        }
    }
}

impl<S: Clone + std::hash::Hash + Eq, M, C: MetricConstructor<M>> Family<S, M, C> {
    /// Access a metric with the given label set, creating it if one does not
    /// yet exist.
    ///
    /// ```
    /// # use prometheus_client::metrics::counter::{Atomic, Counter};
    /// # use prometheus_client::metrics::family::Family;
    /// #
    /// let family = Family::<Vec<(String, String)>, Counter>::default();
    ///
    /// // Will create the metric with label `method="GET"` on first call and
    /// // return a reference.
    /// family.get_or_create(&vec![("method".to_owned(), "GET".to_owned())]).inc();
    ///
    /// // Will return a reference to the existing metric on all subsequent
    /// // calls.
    /// family.get_or_create(&vec![("method".to_owned(), "GET".to_owned())]).inc();
    /// ```
    pub fn get_or_create(&self, label_set: &S) -> MappedRwLockReadGuard<M> {
        if let Ok(metric) =
            RwLockReadGuard::try_map(self.metrics.read(), |metrics| metrics.get(label_set))
        {
            return metric;
        }

        let mut write_guard = self.metrics.write();

        write_guard
            .entry(label_set.clone())
            .or_insert_with(|| self.constructor.new_metric());

        let read_guard = RwLockWriteGuard::downgrade(write_guard);

        RwLockReadGuard::map(read_guard, |metrics| {
            metrics
                .get(label_set)
                .expect("Metric to exist after creating it.")
        })
    }

    /// Access a metric with the given label set, returning None if one
    /// does not yet exist.
    ///
    /// ```
    /// # use prometheus_client::metrics::counter::{Atomic, Counter};
    /// # use prometheus_client::metrics::family::Family;
    /// #
    /// let family = Family::<Vec<(String, String)>, Counter>::default();
    ///
    /// if let Some(metric) = family.get(&vec![("method".to_owned(), "GET".to_owned())]) {
    ///     metric.inc();
    /// };
    /// ```
    pub fn get(&self, label_set: &S) -> Option<MappedRwLockReadGuard<M>> {
        RwLockReadGuard::try_map(self.metrics.read(), |metrics| metrics.get(label_set)).ok()
    }

    /// Remove a label set from the metric family.
    ///
    /// Returns a bool indicating if a label set was removed or not.
    ///
    /// ```
    /// # use prometheus_client::metrics::counter::{Atomic, Counter};
    /// # use prometheus_client::metrics::family::Family;
    /// #
    /// let family = Family::<Vec<(String, String)>, Counter>::default();
    ///
    /// // Will create the metric with label `method="GET"` on first call and
    /// // return a reference.
    /// family.get_or_create(&vec![("method".to_owned(), "GET".to_owned())]).inc();
    ///
    /// // Will return `true`, indicating that the `method="GET"` label set was
    /// // removed.
    /// assert!(family.remove(&vec![("method".to_owned(), "GET".to_owned())]));
    /// ```
    pub fn remove(&self, label_set: &S) -> bool {
        self.metrics.write().remove(label_set).is_some()
    }

    /// Clear all label sets from the metric family.
    ///
    /// ```
    /// # use prometheus_client::metrics::counter::{Atomic, Counter};
    /// # use prometheus_client::metrics::family::Family;
    /// #
    /// let family = Family::<Vec<(String, String)>, Counter>::default();
    ///
    /// // Will create the metric with label `method="GET"` on first call and
    /// // return a reference.
    /// family.get_or_create(&vec![("method".to_owned(), "GET".to_owned())]).inc();
    ///
    /// // Clear the family of all label sets.
    /// family.clear();
    /// ```
    pub fn clear(&self) {
        self.metrics.write().clear()
    }

    pub(crate) fn read(&self) -> RwLockReadGuard<HashMap<S, M>> {
        self.metrics.read()
    }
}

impl<S, M, C: Clone> Clone for Family<S, M, C> {
    fn clone(&self) -> Self {
        Family {
            metrics: self.metrics.clone(),
            constructor: self.constructor.clone(),
        }
    }
}

impl<S, M: TypedMetric, C> TypedMetric for Family<S, M, C> {
    const TYPE: MetricType = <M as TypedMetric>::TYPE;
}

impl<S, M, C> EncodeMetric for Family<S, M, C>
where
    S: Clone + std::hash::Hash + Eq + EncodeLabelSet,
    M: EncodeMetric + TypedMetric,
    C: MetricConstructor<M>,
{
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        let guard = self.read();
        for (label_set, m) in guard.iter() {
            let encoder = encoder.encode_family(label_set)?;
            m.encode(encoder)?;
        }
        Ok(())
    }

    fn metric_type(&self) -> MetricType {
        M::TYPE
    }
}

impl<S, M, C> Family<S, M, C>
where
    S: Clone + std::hash::Hash + Eq,
{
    /// Provides controlled access to the internal metrics storage.
    ///
    /// This method allows performing custom operations on the metrics
    /// while maintaining encapsulation. It takes a closure that can
    /// read from or write to the metrics.
    ///
    /// # Arguments
    ///
    /// * `f` - A closure that takes a reference to the metrics storage
    ///   and returns a value of type `R`.
    ///
    /// # Returns
    ///
    /// Returns the value produced by the closure.
    ///
    /// # Examples
    ///
    /// ```
    /// # use prometheus_client::metrics::counter::Counter;
    /// # use prometheus_client::metrics::family::Family;
    /// #
    /// let family = Family::<String, Counter>::default();
    ///
    /// // Read operation
    /// let count = family.with_metrics(|metrics| {
    ///     metrics.read().len()
    /// });
    ///
    /// // Write operation
    /// family.with_metrics(|metrics| {
    ///     metrics.write().insert("new".to_string(), Counter::default());
    /// });
    /// ```
    pub fn with_metrics<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&RwLock<HashMap<S, M>>) -> R
    {
        f(&self.metrics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::counter::Counter;
    use crate::metrics::histogram::{exponential_buckets, Histogram};

    #[test]
    fn counter_family() {
        let family = Family::<Vec<(String, String)>, Counter>::default();

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

    #[test]
    fn histogram_family() {
        Family::<(), Histogram>::new_with_constructor(|| {
            Histogram::new(exponential_buckets(1.0, 2.0, 10))
        });
    }

    #[test]
    fn histogram_family_with_struct_constructor() {
        struct CustomBuilder {
            custom_start: f64,
        }
        impl MetricConstructor<Histogram> for CustomBuilder {
            fn new_metric(&self) -> Histogram {
                Histogram::new(exponential_buckets(self.custom_start, 2.0, 10))
            }
        }

        let custom_builder = CustomBuilder { custom_start: 1.0 };
        Family::<(), Histogram, CustomBuilder>::new_with_constructor(custom_builder);
    }

    #[test]
    fn counter_family_remove() {
        let family = Family::<Vec<(String, String)>, Counter>::default();

        family
            .get_or_create(&vec![("method".to_string(), "GET".to_string())])
            .inc();

        assert_eq!(
            1,
            family
                .get_or_create(&vec![("method".to_string(), "GET".to_string())])
                .get()
        );

        family
            .get_or_create(&vec![("method".to_string(), "POST".to_string())])
            .inc_by(2);

        assert_eq!(
            2,
            family
                .get_or_create(&vec![("method".to_string(), "POST".to_string())])
                .get()
        );

        // Attempt to remove it twice, showing it really was removed on the
        // first attempt.
        assert!(family.remove(&vec![("method".to_string(), "POST".to_string())]));
        assert!(!family.remove(&vec![("method".to_string(), "POST".to_string())]));

        // This should make a new POST label.
        family
            .get_or_create(&vec![("method".to_string(), "POST".to_string())])
            .inc();

        assert_eq!(
            1,
            family
                .get_or_create(&vec![("method".to_string(), "POST".to_string())])
                .get()
        );

        // GET label should have be untouched.
        assert_eq!(
            1,
            family
                .get_or_create(&vec![("method".to_string(), "GET".to_string())])
                .get()
        );
    }

    #[test]
    fn counter_family_clear() {
        let family = Family::<Vec<(String, String)>, Counter>::default();

        // Create a label and check it.
        family
            .get_or_create(&vec![("method".to_string(), "GET".to_string())])
            .inc();

        assert_eq!(
            1,
            family
                .get_or_create(&vec![("method".to_string(), "GET".to_string())])
                .get()
        );

        // Clear it, then try recreating and checking it again.
        family.clear();

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

    #[test]
    fn test_with_metrics() {
        let family = Family::<String, Counter>::default();

        // Test read operation
        family.get_or_create(&"test".to_string()).inc();
        let count = family.with_metrics(|metrics| {
            metrics.read().get("test").map(|counter| counter.get()).unwrap_or(0)
        });
        assert_eq!(count, 1);

        // Test write operation
        family.with_metrics(|metrics| {
            metrics.write().insert("new".to_string(), Counter::default());
        });
        assert!(family.get(&"new".to_string()).is_some());

        // Test complex operation
        let keys: Vec<String> = family.with_metrics(|metrics| {
            metrics.read().keys().cloned().collect()
        });
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"test".to_string()));
        assert!(keys.contains(&"new".to_string()));

        // Test returning different types
        let key_count: usize = family.with_metrics(|metrics| {
            metrics.read().len()
        });
        assert_eq!(key_count, 2);
    }

    #[test]
    fn test_get() {
        let family = Family::<Vec<(String, String)>, Counter>::default();

        // Test getting a non-existent metric
        let non_existent = family.get(&vec![("method".to_string(), "GET".to_string())]);
        assert!(non_existent.is_none());

        // Create a metric
        family.get_or_create(&vec![("method".to_string(), "GET".to_string())]).inc();

        // Test getting an existing metric
        let existing = family.get(&vec![("method".to_string(), "GET".to_string())]);
        assert!(existing.is_some());
        assert_eq!(existing.unwrap().get(), 1);

        // Test getting a different non-existent metric
        let another_non_existent = family.get(&vec![("method".to_string(), "POST".to_string())]);
        assert!(another_non_existent.is_none());

        // Test modifying the metric through the returned reference
        if let Some(metric) = family.get(&vec![("method".to_string(), "GET".to_string())]) {
            metric.inc();
        }

        // Verify the modification
        let modified = family.get(&vec![("method".to_string(), "GET".to_string())]);
        assert_eq!(modified.unwrap().get(), 2);

        // Test with a different label set type
        let string_family = Family::<String, Counter>::default();
        string_family.get_or_create(&"test".to_string()).inc();

        let string_metric = string_family.get(&"test".to_string());
        assert!(string_metric.is_some());
        assert_eq!(string_metric.unwrap().get(), 1);

        let non_existent_string = string_family.get(&"non_existent".to_string());
        assert!(non_existent_string.is_none());
    }
}
