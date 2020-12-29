use owning_ref::OwningRef;

use std::collections::HashMap;
use std::sync::{Arc, RwLock, RwLockReadGuard};

/// Representation of the OpenMetrics *MetricFamily* data type.
///
/// A [`MetricFamily`] is a set of metrics with the same name, help text and
/// type, differentiated by their label values thus spanning a multidimensional
/// space.
///
/// # Generic over the label set
///
/// A [`MetricFamily`] is generic over the label type. For convenience one might
/// choose a `Vec<(String, String)>`, for performance one might define a custom
/// type.
///
/// ## Examples
///
/// ### [`MetricFamily`] with `Vec<(String, String)>` for convenience
///
/// ```
/// # use std::sync::atomic::AtomicU64;
/// # use open_metrics_client::counter::{Atomic, Counter};
/// # use open_metrics_client::encoding::text::encode;
/// # use open_metrics_client::family::MetricFamily;
/// # use open_metrics_client::registry::{Descriptor, Registry};
/// #
/// # let mut registry = Registry::new();
/// let family = MetricFamily::<Vec<(String, String)>, Counter<AtomicU64>>::new();
/// # registry.register(
/// #   Descriptor::new("counter", "This is my counter.", "my_counter"),
/// #   family.clone(),
/// # );
///
/// // Record a single HTTP GET request.
/// family.get_or_create(&vec![("method".to_owned(), "GET".to_owned())]).inc();
///
/// # // Encode all metrics in the registry in the text format.
/// # let mut buffer = vec![];
/// # encode::<_, _>(&mut buffer, &registry).unwrap();
/// #
/// # let expected = "# HELP my_counter This is my counter.\n".to_owned() +
/// #                "# TYPE my_counter counter\n" +
/// #                "my_counter_total{method=\"GET\"} 1\n" +
/// #                "# EOF\n";
/// # assert_eq!(expected, String::from_utf8(buffer).unwrap());
/// ```
///
/// ### [`MetricFamily`] with custom type for performance
///
/// ```
/// # use std::sync::atomic::AtomicU64;
/// # use open_metrics_client::counter::{Atomic, Counter};
/// # use open_metrics_client::encoding::text::encode;
/// # use open_metrics_client::family::MetricFamily;
/// # use open_metrics_client::registry::{Descriptor, Registry};
/// # use open_metrics_client::encoding::text::Encode;
/// # use std::io::Write;
/// #
/// # let mut registry = Registry::new();
/// #[derive(Clone, Hash, PartialEq, Eq)]
/// struct Labels {
///   method: Method,
/// };
///
/// #[derive(Clone, Hash, PartialEq, Eq)]
/// enum Method {
///   Get,
///   Put,
/// };
///
/// # impl Encode for Labels {
/// #   fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
/// #     let method = match self.method {
/// #         Method::Get => {
/// #             b"method=\"GET\""
/// #         }
/// #         Method::Put => {
/// #             b"method=\"PUT\""
/// #         }
/// #     };
/// #     writer.write(method).map(|_| ())
/// #   }
/// # }
/// #
/// let family = MetricFamily::<Labels, Counter<AtomicU64>>::new();
/// # registry.register(
/// #   Descriptor::new("counter", "This is my counter.", "my_counter"),
/// #   family.clone(),
/// # );
///
/// // Record a single HTTP GET request.
/// family.get_or_create(&Labels { method: Method::Get }).inc();
/// #
/// # // Encode all metrics in the registry in the text format.
/// # let mut buffer = vec![];
/// # encode::<_, _>(&mut buffer, &registry).unwrap();
///
/// # let expected = "# HELP my_counter This is my counter.\n".to_owned() +
/// #                "# TYPE my_counter counter\n" +
/// #                "my_counter_total{method=\"GET\"} 1\n" +
/// #                "# EOF\n";
/// # assert_eq!(expected, String::from_utf8(buffer).unwrap());
/// ```
// TODO: Rename to Family.
pub struct MetricFamily<S, M> {
    metrics: Arc<RwLock<HashMap<S, M>>>,
}

impl<S: Clone + std::hash::Hash + Eq, M: Default> MetricFamily<S, M> {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(RwLock::new(Default::default())),
        }
    }

    pub fn get_or_create(&self, sample_set: &S) -> OwningRef<RwLockReadGuard<HashMap<S, M>>, M> {
        let read_guard = self.metrics.read().unwrap();
        if let Ok(metric) =
            OwningRef::new(read_guard).try_map(|metrics| metrics.get(sample_set).ok_or(()))
        {
            return metric;
        }

        let mut write_guard = self.metrics.write().unwrap();
        write_guard.insert(sample_set.clone(), Default::default());

        drop(write_guard);

        let read_guard = self.metrics.read().unwrap();
        return OwningRef::new(read_guard).map(|metrics| metrics.get(sample_set).unwrap());
    }

    pub(crate) fn read<'a>(&'a self) -> RwLockReadGuard<'a, HashMap<S, M>> {
        self.metrics.read().unwrap()
    }
}

impl<S, M> Clone for MetricFamily<S, M> {
    fn clone(&self) -> Self {
        MetricFamily {
            metrics: self.metrics.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counter::Counter;
    use std::sync::atomic::AtomicU64;

    #[test]
    fn counter_family() {
        let family = MetricFamily::<Vec<(String, String)>, Counter<AtomicU64>>::new();

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
}
