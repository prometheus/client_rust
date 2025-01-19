//! Metric type implementations.

pub mod counter;
pub mod exemplar;
pub mod family;
pub mod gauge;
pub mod histogram;
pub mod info;
#[cfg(feature = "summary")]
pub mod summary;

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
    #[cfg(feature = "summary")]
    Summary,
    Unknown,
    // Not (yet) supported metric types.
    //
    // GaugeHistogram,
    // StateSet,
}

impl MetricType {
    /// Returns the given metric type's str representation.
    pub fn as_str(&self) -> &str {
        match self {
            MetricType::Counter => "counter",
            MetricType::Gauge => "gauge",
            MetricType::Histogram => "histogram",
            MetricType::Info => "info",
            #[cfg(feature = "summary")]
            MetricType::Summary => "summary",
            MetricType::Unknown => "unknown",
        }
    }
}
