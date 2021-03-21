use super::counter::{self, Counter};
use owning_ref::OwningRef;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock, RwLockReadGuard};

pub struct Exemplar<S, V> {
    pub(crate) label_set: S,
    pub(crate) value: V,
}

/////////////////////////////////////////////////////////////////////////////////
// Counter

pub struct CounterWithExemplar<S, N = u64, A = AtomicU64> {
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
    pub fn get(
        &self,
    ) -> (
        N,
        OwningRef<RwLockReadGuard<CounterWithExemplarInner<S, N, A>>, Option<Exemplar<S, N>>>,
    ) {
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
