#![forbid(unsafe_code)]
#![deny(unused)]
#![deny(dead_code)]

//! Client library implementing the [Open Metrics
//! specification](https://github.com/OpenObservability/OpenMetrics) allowing
//! users to natively instrument applications.
//!
//! # Examples
//!
//! ```
//! # use std::sync::atomic::AtomicU64;
//! # use open_metrics_client::registry::{Descriptor, Registry};
//! # use open_metrics_client::counter::Counter;
//! # use open_metrics_client::encoding::text::encode;
//! #
//! // Create registry and counter and register the latter with the former.
//! let mut registry = Registry::default();
//! let counter = Counter::<AtomicU64>::new();
//! registry.register(
//!   Descriptor::new("counter", "This is my counter.", "my_counter"),
//!   counter.clone(),
//! );
//!
//! // Record an observation by increasing the counter.
//! counter.inc();
//!
//! // Encode all metrics in the registry in the text format.
//! let mut buffer = vec![];
//! encode::<_, _>(&mut buffer, &registry).unwrap();
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

pub mod counter;
pub mod encoding;
pub mod family;
pub mod gauge;
pub mod histogram;
pub mod registry;
