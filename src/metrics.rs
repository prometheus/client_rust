pub mod counter;
pub mod family;
pub mod gauge;
pub mod histogram;

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
