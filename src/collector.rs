//! Metric collector implementation.
//!
//! See [`Collector`] for details.

use crate::encoding::DescriptorEncoder;

/// The [`Collector`] abstraction allows users to provide additional metrics and
/// their description on each scrape.
///
/// An example use-case is an exporter that retrieves a set of operating system metrics
/// ad-hoc on each scrape.
///
/// Register a [`Collector`] with a [`Registry`](crate::registry::Registry) via
/// [`Registry::register_collector`](crate::registry::Registry::register_collector).
///
/// ```
/// # use prometheus_client::metrics::counter::ConstCounter;
/// # use prometheus_client::collector::Collector;
/// # use prometheus_client::encoding::{DescriptorEncoder, EncodeMetric};
/// #
/// #[derive(Debug)]
/// struct MyCollector {}
///
/// impl Collector for MyCollector {
///     fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
///         let counter = ConstCounter::new(42u64);
///         let metric_encoder = encoder.encode_descriptor(
///             "my_counter",
///             "some help",
///             None,
///             counter.metric_type(),
///         )?;
///         counter.encode(metric_encoder)?;
///         Ok(())
///     }
/// }
/// ```
pub trait Collector: std::fmt::Debug + Send + Sync + 'static {
    /// Once the [`Collector`] is registered, this method is called on each scrape.
    fn encode(&self, encoder: DescriptorEncoder) -> Result<(), std::fmt::Error>;
}
