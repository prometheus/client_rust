//! Module implementing an Open Metrics exemplars for counters and histograms.
//!
//! See [`CounterWithExemplar`] and [`HistogramWithExemplars`] for details.

use super::counter::{self, Counter};
use super::histogram::Histogram;
use owning_ref::OwningRef;
use std::collections::HashMap;
#[cfg(any(target_arch = "mips", target_arch = "powerpc"))]
use std::sync::atomic::AtomicU32;
#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock, RwLockReadGuard};

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
#[cfg(not(any(target_arch = "mips", target_arch = "powerpc")))]
pub struct CounterWithExemplar<S, N = u64, A = AtomicU64> {
    pub(crate) inner: Arc<RwLock<CounterWithExemplarInner<S, N, A>>>,
}

#[cfg(any(target_arch = "mips", target_arch = "powerpc"))]
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
        let mut inner = self.inner.write().expect("Lock not to be poisoned.");

        inner.exemplar = label_set.map(|label_set| Exemplar {
            label_set,
            value: v.clone(),
        });

        inner.counter.inc_by(v)
    }

    /// Get the current value of the [`CounterWithExemplar`] as well as its
    /// [`Exemplar`] if any.
    pub fn get(&self) -> (N, RwLockGuardedCounterWithExemplar<S, N, A>) {
        let inner = self.inner.read().expect("Lock not to be poisoned.");
        let value = inner.counter.get();
        let exemplar = OwningRef::new(inner).map(|inner| &inner.exemplar);
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
    pub fn inner(&self) -> OwningRef<RwLockReadGuard<CounterWithExemplarInner<S, N, A>>, A> {
        OwningRef::new(self.inner.read().expect("Lock not to be poisoned."))
            .map(|inner| inner.counter.inner())
    }
}

type RwLockGuardedCounterWithExemplar<'a, S, N, A> =
    OwningRef<RwLockReadGuard<'a, CounterWithExemplarInner<S, N, A>>, Option<Exemplar<S, N>>>;

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
pub struct HistogramWithExemplars<S> {
    // TODO: Not ideal, as Histogram has a Mutex as well.
    pub(crate) inner: Arc<RwLock<HistogramWithExemplarsInner<S>>>,
}

impl<S> Clone for HistogramWithExemplars<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pub struct HistogramWithExemplarsInner<S> {
    pub(crate) exemplars: HashMap<usize, Exemplar<S, f64>>,
    pub(crate) histogram: Histogram,
}

impl<S> HistogramWithExemplars<S> {
    pub fn new(buckets: impl Iterator<Item = f64>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HistogramWithExemplarsInner {
                exemplars: Default::default(),
                histogram: Histogram::new(buckets),
            })),
        }
    }

    pub fn observe(&self, v: f64, label_set: Option<S>) {
        let mut inner = self.inner.write().expect("Lock not to be poisoned.");
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
        self.inner.read().expect("Lock not to be poisoned.")
    }
}
