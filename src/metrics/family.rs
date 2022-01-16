//! Module implementing an Open Metrics metric family.
//!
//! See [`Family`] for details.

use super::{MetricType, TypedMetric};
use owning_ref::OwningRef;
use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard};

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
/// # use prometheus_client::registry::{Descriptor, Registry};
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
/// # let mut buffer = vec![];
/// # encode(&mut buffer, &registry).unwrap();
/// #
/// # let expected = "# HELP my_counter This is my counter.\n".to_owned() +
/// #                "# TYPE my_counter counter\n" +
/// #                "my_counter_total{method=\"GET\"} 1\n" +
/// #                "# EOF\n";
/// # assert_eq!(expected, String::from_utf8(buffer).unwrap());
/// ```
///
/// ### [`Family`] with custom type for performance and/or type safety
///
/// Using `Encode` derive macro to generate
/// [`Encode`](crate::encoding::text::Encode) implementation.
///
/// ```
/// # use prometheus_client::encoding::text::Encode;
/// # use prometheus_client::encoding::text::encode;
/// # use prometheus_client::metrics::counter::{Atomic, Counter};
/// # use prometheus_client::metrics::family::Family;
/// # use prometheus_client::registry::{Descriptor, Registry};
/// # use std::io::Write;
/// #
/// # let mut registry = Registry::default();
/// #[derive(Clone, Hash, PartialEq, Eq, Encode)]
/// struct Labels {
///   method: Method,
/// };
///
/// #[derive(Clone, Hash, PartialEq, Eq, Encode)]
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
/// # let mut buffer = vec![];
/// # encode(&mut buffer, &registry).unwrap();
/// #
/// # let expected = "# HELP my_counter This is my counter.\n".to_owned() +
/// #                "# TYPE my_counter counter\n" +
/// #                "my_counter_total{method=\"GET\"} 1\n" +
/// #                "# EOF\n";
/// # assert_eq!(expected, String::from_utf8(buffer).unwrap());
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
    fn new_metric(&self) -> M;
}

/// In cases in which the explicit type of the metric is not required, it is
/// posible to directly provide a closure even if it captures variables.
///
/// ```
/// # use prometheus_client::metrics::family::{Family};
/// # use prometheus_client::metrics::histogram::Histogram;
/// let custom_buckets = vec![0.0, 10.0, 100.0];
/// let metric = Family::<(), Histogram, _>::new_with_constructor(|| {
///     Histogram::new(custom_buckets.clone().into_iter())
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
    pub fn get_or_create(&self, label_set: &S) -> OwningRef<RwLockReadGuard<HashMap<S, M>>, M> {
        let read_guard = self.metrics.read().expect("Lock not to be poisoned.");
        if let Ok(metric) =
            OwningRef::new(read_guard).try_map(|metrics| metrics.get(label_set).ok_or(()))
        {
            return metric;
        }

        let mut write_guard = self.metrics.write().unwrap();
        write_guard.insert(label_set.clone(), self.constructor.new_metric());

        drop(write_guard);

        let read_guard = self.metrics.read().unwrap();
        OwningRef::new(read_guard).map(|metrics| {
            metrics
                .get(label_set)
                .expect("Metric to exist after creating it.")
        })
    }

    pub(crate) fn read(&self) -> RwLockReadGuard<HashMap<S, M>> {
        self.metrics.read().unwrap()
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
}
