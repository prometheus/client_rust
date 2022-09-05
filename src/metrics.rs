//! Metric type implementations.

pub mod counter;
pub mod exemplar;
pub mod family;
pub mod gauge;
pub mod histogram;
pub mod info;

/// A metric that is aware of its Open Metrics metric type.
pub trait TypedMetric {
    /// The OpenMetrics metric type.
    const TYPE: MetricType = MetricType::Unknown;
}

/// OpenMetrics metric type.
#[derive(Clone, Copy, Debug)]
#[allow(missing_docs)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Info,
    Unknown,
    // Not (yet) supported metric types.
    //
    // GaugeHistogram,
    // StateSet,
    // Summary
}
