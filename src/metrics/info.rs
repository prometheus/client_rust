//! Module implementing an Open Metrics info metric.
//!
//! See [`Info`] for details.

use crate::metrics::{MetricType, TypedMetric};

/// Open Metrics [`Info`] metric "to expose textual information which SHOULD NOT
/// change during process lifetime".
///
/// ```
/// # use prometheus_client::metrics::info::Info;
///
/// let _info = Info::new(vec![("os", "GNU/linux")]);
/// ```
pub struct Info<S>(pub(crate) S);

impl<S> Info<S> {
    pub fn new(label_set: S) -> Self {
        Self(label_set)
    }
}

impl<S> TypedMetric for Info<S> {
    const TYPE: MetricType = MetricType::Info;
}
