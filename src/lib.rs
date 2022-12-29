#![deny(dead_code)]
#![deny(missing_docs)]
#![deny(unused)]
#![forbid(unsafe_code)]
#![warn(missing_debug_implementations)]

//! Client library implementation of the [Open Metrics
//! specification](https://github.com/OpenObservability/OpenMetrics). Allows
//! developers to instrument applications and thus enables operators to monitor
//! said applications with monitoring systems like
//! [Prometheus](https://prometheus.io/).
//!
//! # Examples
//!
//! ```
//! use prometheus_client::encoding::{EncodeLabelSet, EncodeLabelValue};
//! use prometheus_client::encoding::text::encode;
//! use prometheus_client::metrics::counter::{Atomic, Counter};
//! use prometheus_client::metrics::family::Family;
//! use prometheus_client::registry::Registry;
//! use std::io::Write;
//!
//! // Create a metric registry.
//! //
//! // Note the angle brackets to make sure to use the default (dynamic
//! // dispatched boxed metric) for the generic type parameter.
//! let mut registry = <Registry>::default();
//!
//! // Define a type representing a metric label set, i.e. a key value pair.
//! //
//! // You could as well use `(String, String)` to represent a label set,
//! // instead of the custom type below.
//! #[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
//! struct Labels {
//!   // Use your own enum types to represent label values.
//!   method: Method,
//!   // Or just a plain string.
//!   path: String,
//! };
//!
//! #[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelValue)]
//! enum Method {
//!   GET,
//!   PUT,
//! };
//!
//! // Create a sample counter metric family utilizing the above custom label
//! // type, representing the number of HTTP requests received.
//! let http_requests = Family::<Labels, Counter>::default();
//!
//! // Register the metric family with the registry.
//! registry.register(
//!   // With the metric name.
//!   "http_requests",
//!   // And the metric help text.
//!   "Number of HTTP requests received",
//!   http_requests.clone(),
//! );
//!
//! // Somewhere in your business logic record a single HTTP GET request.
//! http_requests.get_or_create(
//!     &Labels { method: Method::GET, path: "/metrics".to_string() }
//! ).inc();
//!
//! // When a monitoring system like Prometheus scrapes the local node, encode
//! // all metrics in the registry in the text format, and send the encoded
//! // metrics back.
//! let mut buffer = String::new();
//! encode(&mut buffer, &registry).unwrap();
//!
//! let expected = "# HELP http_requests Number of HTTP requests received.\n".to_owned() +
//!                "# TYPE http_requests counter\n" +
//!                "http_requests_total{method=\"GET\",path=\"/metrics\"} 1\n" +
//!                "# EOF\n";
//! assert_eq!(expected, buffer);
//! ```
//! See [examples] directory for more.
//!
//! [examples]: https://github.com/prometheus/client_rust/tree/master/examples

pub mod collector;
pub mod encoding;
pub mod metrics;
pub mod registry;

/// Represents either borrowed or owned data.
///
/// In contrast to [`std::borrow::Cow`] does not require
/// [`std::borrow::ToOwned`] or [`Clone`]respectively.
///
/// Needed for [`collector::Collector`].
#[derive(Debug)]
pub enum MaybeOwned<'a, T> {
    /// Owned data
    Owned(T),
    /// Borrowed data
    Borrowed(&'a T),
}

impl<'a, T> std::ops::Deref for MaybeOwned<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(t) => t,
            Self::Borrowed(t) => t,
        }
    }
}
