//! Metric type implementations.

pub mod counter;
pub mod family;
pub mod gauge;
pub mod histogram;

/// A metric that is aware of its Open Metrics metric type.
pub trait TypedMetric {
    const TYPE: MetricType = MetricType::Unknown;
}

#[derive(Clone, Copy)]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
    Unknown,
    // Not (yet) supported metric types.
    //
    // GaugeHistogram,
    // Info,
    // StateSet,
    // Summary
}
