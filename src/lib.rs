#![forbid(unsafe_code)]
#![deny(unused)]
#![deny(dead_code)]

//! Client library implementation of the [Open Metrics
//! specification](https://github.com/OpenObservability/OpenMetrics). Allows
//! developers to instrument applications and thus enables operators to monitor
//! said applications with monitoring systems like
//! [Prometheus](https://prometheus.io/).
//!
//! # Examples
//!
//! ```
//! # use std::sync::atomic::AtomicU64;
//! # use open_metrics_client::registry::{Descriptor, Registry};
//! # use open_metrics_client::metrics::counter::Counter;
//! # use open_metrics_client::encoding::text::encode;
//! #
//! // Create registry and counter and register the latter with the former.
//! let mut registry = Registry::default();
//! let counter = Counter::<AtomicU64>::new();
//! registry.register(
//!   "my_counter",
//!   "This is my counter",
//!   counter.clone(),
//! );
//!
//! // Record an observation by increasing the counter.
//! counter.inc();
//!
//! // Encode all metrics in the registry in the text format.
//! let mut buffer = vec![];
//! encode(&mut buffer, &registry).unwrap();
//!
//! let expected = "# HELP my_counter This is my counter.\n".to_owned() +
//!                "# TYPE my_counter counter\n" +
//!                "my_counter_total 1\n" +
//!                "# EOF\n";
//! assert_eq!(expected, String::from_utf8(buffer).unwrap());
//! ```
//! See [examples] directory for more.
//!
//! [examples]: https://github.com/mxinden/rust-open-metrics-client/tree/master/examples

pub mod encoding;
pub mod metrics;
pub mod registry;
