#![deny(dead_code)]
#![deny(missing_docs)]
#![deny(unused)]
#![forbid(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! This crate provides a procedural macro to derive
//! auxiliary traits for the
//! [`prometheus_client`](https://docs.rs/prometheus-client/latest/prometheus_client/)
mod registrant;

use proc_macro::TokenStream as TokenStream1;
use proc_macro2::TokenStream as TokenStream2;
use syn::Error;

type Result<T> = std::result::Result<T, Error>;

#[proc_macro_derive(Registrant, attributes(registrant))]
/// Derives the `prometheus_client::registry::Registrant` trait implementation for a struct.
/// ```rust
/// use prometheus_client::metrics::counter::Counter;
/// use prometheus_client::metrics::gauge::Gauge;
/// use prometheus_client::registry::{Registry, Registrant as _};
/// use prometheus_client_derive::Registrant;
///
/// #[derive(Registrant)]
/// struct Server {
///     /// Number of HTTP requests received
///     /// from the client
///     requests: Counter,
///     /// Memory usage in bytes
///     /// of the server
///     #[registrant(unit = "bytes")]
///     memory_usage: Gauge,
/// }
///
/// let mut registry = Registry::default();
/// let server = Server {
///     requests: Counter::default(),
///     memory_usage: Gauge::default(),
/// };
/// server.register(&mut registry);
/// ```
///
/// There are several field attributes:
/// - `#[registrant(rename = "...")]`: Renames the metric.
/// - `#[registrant(unit = "...")]`: Sets the unit of the metric.
/// - `#[registrant(skip)]`: Skips the field and does not register it.
pub fn registrant_derive(input: TokenStream1) -> TokenStream1 {
    match registrant::registrant_impl(input.into()) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}
