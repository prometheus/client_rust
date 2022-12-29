//! Metric collector implementation.
//!
//! See [`Collector`] for details.

use std::borrow::Cow;

use crate::{
    registry::{Descriptor, LocalMetric},
    MaybeOwned,
};

/// The [`Collector`] abstraction allows users to provide additional metrics and
/// their description on each scrape.
///
/// An example use-case is an exporter that retrieves a set of operating system metrics
/// ad-hoc on each scrape.
///
/// Register a [`Collector`] with a [`Registry`](crate::registry::Registry) via
/// [`Registry::register_collector`](crate::registry::Registry::register_collector).
pub trait Collector: std::fmt::Debug + Send + Sync + 'static {
    /// Once the [`Collector`] is registered, this method is called on each scrape.
    ///
    /// Note that the return type allows you to either return owned (convenient)
    /// or borrowed (performant) descriptions and metrics.
    #[allow(clippy::type_complexity)]
    fn collect<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (Cow<'a, Descriptor>, MaybeOwned<'a, Box<dyn LocalMetric>>)> + 'a>;
}
