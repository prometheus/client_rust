//! Module implementing an Open Metrics exemplars for counters and histograms.
//!
//! See [`CounterWithExemplar`] and [`HistogramWithExemplars`] for details.

use crate::encoding::{
    EncodeCounterValue, EncodeExemplarValue, EncodeLabelSet, EncodeMetric, MetricEncoder,
};

use super::counter::{self, Counter};
use super::histogram::Histogram;
use super::{MetricType, TypedMetric};
use parking_lot::{MappedRwLockReadGuard, RwLock, RwLockReadGuard};
use std::collections::HashMap;
#[cfg(any(target_arch = "mips", target_arch = "powerpc"))]
use std::sync::atomic::AtomicU32;
#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

/// An OpenMetrics exemplar.
#[derive(Debug)]
pub struct Exemplar<S, V> {
    pub(crate) label_set: S,
    pub(crate) value: V,
}

/////////////////////////////////////////////////////////////////////////////////
// Counter

/// Open Metrics [`Counter`] with an [`Exemplar`] to both measure discrete
/// events and track references to data outside of the metric set.
///
/// ```
/// # use prometheus_client::metrics::exemplar::CounterWithExemplar;
/// let counter_with_exemplar = CounterWithExemplar::<Vec<(String, String)>>::default();
/// counter_with_exemplar.inc_by(1, Some(vec![("user_id".to_string(), "42".to_string())]));
/// let _value: (u64, _) = counter_with_exemplar.get();
/// ```
/// You can also use exemplars with families. Just wrap the exemplar in a Family.
/// ```
/// # use prometheus_client::metrics::exemplar::CounterWithExemplar;
/// # use prometheus_client::metrics::histogram::exponential_buckets;
/// # use prometheus_client::metrics::family::Family;
/// # use prometheus_client_derive_encode::EncodeLabelSet;
/// #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug, Default)]
/// pub struct ResultLabel {
///     pub result: String,
/// }
///
/// #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug, Default)]
/// pub struct TraceLabel {
///     pub trace_id: String,
/// }
///
/// let latency: Family<ResultLabel, CounterWithExemplar<TraceLabel>> = Family::default();
///
/// latency
///     .get_or_create(&ResultLabel {
///         result: "success".to_owned(),
///     })
///     .inc_by(
///         1,
///         Some(TraceLabel {
///             trace_id: "3a2f90c9f80b894f".to_owned(),
///         }),
///     );
/// ```
#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
#[derive(Debug)]
pub struct CounterWithExemplar<S, N = u64, A = AtomicU64> {
    pub(crate) inner: Arc<RwLock<CounterWithExemplarInner<S, N, A>>>,
}

impl<S> TypedMetric for CounterWithExemplar<S> {
    const TYPE: MetricType = MetricType::Counter;
}

/// Open Metrics [`Counter`] with an [`Exemplar`] to both measure discrete
/// events and track references to data outside of the metric set.
#[cfg(any(target_arch = "mips", target_arch = "powerpc"))]
#[derive(Debug)]
pub struct CounterWithExemplar<S, N = u32, A = AtomicU32> {
    pub(crate) inner: Arc<RwLock<CounterWithExemplarInner<S, N, A>>>,
}

impl<S, N, A> Clone for CounterWithExemplar<S, N, A> {
    fn clone(&self) -> Self {
        CounterWithExemplar {
            inner: self.inner.clone(),
        }
    }
}

/// An OpenMetrics [`Counter`] in combination with an OpenMetrics [`Exemplar`].
#[derive(Debug)]
pub struct CounterWithExemplarInner<S, N, A> {
    pub(crate) exemplar: Option<Exemplar<S, N>>,
    pub(crate) counter: Counter<N, A>,
}

impl<S, N, A: Default> Default for CounterWithExemplar<S, N, A> {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(CounterWithExemplarInner {
                exemplar: None,
                counter: Default::default(),
            })),
        }
    }
}

impl<S, N: Clone, A: counter::Atomic<N>> CounterWithExemplar<S, N, A> {
    // TODO: Implement `fn inc`. Problematic right now as one can not produce
    // value `1` of type `N`.

    /// Increase the [`CounterWithExemplar`] by `v`, updating the [`Exemplar`]
    /// if a label set is provided, returning the previous value.
    pub fn inc_by(&self, v: N, label_set: Option<S>) -> N {
        let mut inner = self.inner.write();

        inner.exemplar = label_set.map(|label_set| Exemplar {
            label_set,
            value: v.clone(),
        });

        inner.counter.inc_by(v)
    }

    /// Get the current value of the [`CounterWithExemplar`] as well as its
    /// [`Exemplar`] if any.
    pub fn get(&self) -> (N, MappedRwLockReadGuard<Option<Exemplar<S, N>>>) {
        let inner = self.inner.read();
        let value = inner.counter.get();
        let exemplar = RwLockReadGuard::map(inner, |inner| &inner.exemplar);
        (value, exemplar)
    }

    /// Exposes the inner atomic type of the [`CounterWithExemplar`].
    ///
    /// This should only be used for advanced use-cases which are not directly
    /// supported by the library.
    ///
    /// The caller of this function has to uphold the property of an Open
    /// Metrics counter namely that the value is monotonically increasing, i.e.
    /// either stays the same or increases.
    pub fn inner(&self) -> MappedRwLockReadGuard<A> {
        RwLockReadGuard::map(self.inner.read(), |inner| inner.counter.inner())
    }
}

// TODO: S, V, N, A are hard to grasp.
impl<S, N, A> EncodeMetric for crate::metrics::exemplar::CounterWithExemplar<S, N, A>
where
    S: EncodeLabelSet,
    N: EncodeCounterValue + EncodeExemplarValue + Clone,
    A: counter::Atomic<N>,
{
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        let (value, exemplar) = self.get();
        encoder.encode_counter(&value, exemplar.as_ref())
    }

    fn metric_type(&self) -> MetricType {
        Counter::<N, A>::TYPE
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Histogram

/// Open Metrics [`Histogram`] to both measure distributions of discrete events.
/// and track references to data outside of the metric set.
///
/// ```
/// # use prometheus_client::metrics::exemplar::HistogramWithExemplars;
/// # use prometheus_client::metrics::histogram::exponential_buckets;
/// let histogram = HistogramWithExemplars::new(exponential_buckets(1.0, 2.0, 10));
/// histogram.observe(4.2, Some(vec![("user_id".to_string(), "42".to_string())]));
/// ```
/// You can also use exemplars with families. Just wrap the exemplar in a Family.
/// ```
/// # use prometheus_client::metrics::exemplar::HistogramWithExemplars;
/// # use prometheus_client::metrics::histogram::exponential_buckets;
/// # use prometheus_client::metrics::family::Family;
/// # use prometheus_client::encoding::EncodeLabelSet;
/// #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug, Default)]
/// pub struct ResultLabel {
///     pub result: String,
/// }
///
/// #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug, Default)]
/// pub struct TraceLabel {
///     pub trace_id: String,
/// }
///
/// let latency: Family<ResultLabel, HistogramWithExemplars<TraceLabel>> =
///     Family::new_with_constructor(|| {
///         HistogramWithExemplars::new(exponential_buckets(1.0, 2.0, 10))
///     });
///
/// latency
///     .get_or_create(&ResultLabel {
///         result: "success".to_owned(),
///     })
///     .observe(
///         0.001345422,
///         Some(TraceLabel {
///             trace_id: "3a2f90c9f80b894f".to_owned(),
///         }),
///     );
/// ```
#[derive(Debug)]
pub struct HistogramWithExemplars<S> {
    // TODO: Not ideal, as Histogram has a Mutex as well.
    pub(crate) inner: Arc<RwLock<HistogramWithExemplarsInner<S>>>,
}

impl<S> TypedMetric for HistogramWithExemplars<S> {
    const TYPE: MetricType = MetricType::Histogram;
}

impl<S> Clone for HistogramWithExemplars<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// An OpenMetrics [`Histogram`] in combination with an OpenMetrics [`Exemplar`].
#[derive(Debug)]
pub struct HistogramWithExemplarsInner<S> {
    pub(crate) exemplars: HashMap<usize, Exemplar<S, f64>>,
    pub(crate) histogram: Histogram,
}

impl<S> HistogramWithExemplars<S> {
    /// Create a new [`HistogramWithExemplars`].
    pub fn new(buckets: impl Iterator<Item = f64>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HistogramWithExemplarsInner {
                exemplars: Default::default(),
                histogram: Histogram::new(buckets),
            })),
        }
    }

    /// Observe the given value, optionally providing a label set and thus
    /// setting the [`Exemplar`] value.
    pub fn observe(&self, v: f64, label_set: Option<S>) {
        let mut inner = self.inner.write();
        let bucket = inner.histogram.observe_and_bucket(v);
        if let (Some(bucket), Some(label_set)) = (bucket, label_set) {
            inner.exemplars.insert(
                bucket,
                Exemplar {
                    label_set,
                    value: v,
                },
            );
        }
    }

    pub(crate) fn inner(&self) -> RwLockReadGuard<HistogramWithExemplarsInner<S>> {
        self.inner.read()
    }
}

impl<S: EncodeLabelSet> EncodeMetric for HistogramWithExemplars<S> {
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        let inner = self.inner();
        let (sum, count, buckets) = inner.histogram.get();
        encoder.encode_histogram(sum, count, &buckets, Some(&inner.exemplars))
    }

    fn metric_type(&self) -> MetricType {
        Histogram::TYPE
    }
}
