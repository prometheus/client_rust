//! Answering scrapes over HTTP, for example on a `/metrics` endpoint.
//!
//! ```
//! use prometheus_client::encoding::negotiate::Format;
//! use prometheus_client::http::ScrapeHandler;
//! use prometheus_client::metrics::counter::Counter;
//! use prometheus_client::registry::Registry;
//!
//! let mut registry = Registry::default();
//! let counter: Counter = Counter::default();
//! registry.register("my_counter", "This is my counter", counter.clone());
//!
//! // Build the handler once, for example with the state of your HTTP server.
//! let handler = ScrapeHandler::builder()
//!     .with_formats(&[Format::OpenMetricsText1_0_0, Format::PrometheusText0_0_4])
//!     .build();
//!
//! // The headers of the request, as your HTTP framework gives them.
//! let request_headers = [("accept", "application/openmetrics-text;version=1.0.0")];
//!
//! match handler.handle(&registry, |name| {
//!     request_headers
//!         .iter()
//!         .find(|(key, _)| key.eq_ignore_ascii_case(name))
//!         .map(|(_, value)| *value)
//! }) {
//!     Ok(response) => {
//!         // Copy these into the response type of your HTTP framework.
//!         assert_eq!(response.status(), 200);
//!         for (name, value) in response.headers() {
//!             println!("{name}: {value}");
//!         }
//!         let body = response.into_body();
//!         assert!(!body.is_empty());
//!     }
//!     Err(err) => {
//!         // Log `err` and respond with `err.status()`.
//!         eprintln!("{err}");
//!     }
//! }
//! ```

use std::fmt;

use crate::encoding::negotiate::{negotiate, EncodeError, Format};
use crate::registry::Registry;

/// The response to a scrape, as built by [`ScrapeHandler::handle`].
///
/// Copy the status, the headers and the body into the response type of your
/// HTTP framework.
#[derive(Debug)]
pub struct ScrapeResponse {
    status: u16,
    headers: [(&'static str, &'static str); 2],
    body: Vec<u8>,
}

impl ScrapeResponse {
    /// The HTTP status code.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// The headers to respond with, as `(name, value)` pairs with lowercase names.
    ///
    /// Each name appears at most once, so the headers can be copied either by
    /// inserting or by appending them.
    pub fn headers(&self) -> impl Iterator<Item = (&str, &str)> {
        self.headers.iter().copied()
    }

    /// The encoded metrics, to respond with as the body.
    pub fn into_body(self) -> Vec<u8> {
        self.body
    }
}

/// The error returned by [`ScrapeHandler::handle`] when encoding the metrics
/// fails. Its [`source`](std::error::Error::source) is the [`EncodeError`].
#[derive(Debug)]
pub struct ScrapeError {
    status: u16,
    source: EncodeError,
}

impl ScrapeError {
    /// The HTTP status code to respond with.
    pub fn status(&self) -> u16 {
        self.status
    }
}

impl fmt::Display for ScrapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("failed to handle the scrape")
    }
}

impl std::error::Error for ScrapeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Builds the responses to scrapes, in the format the client asks for in its
/// `Accept` header.
///
/// Build it once, for example with the state of your HTTP server, and call
/// [`ScrapeHandler::handle`] on every scrape.
#[derive(Clone, Debug)]
pub struct ScrapeHandler {
    offered: Vec<Format>,
}

impl Default for ScrapeHandler {
    fn default() -> Self {
        ScrapeHandlerBuilder::new().build()
    }
}

impl ScrapeHandler {
    /// A builder for a [`ScrapeHandler`].
    pub fn builder() -> ScrapeHandlerBuilder {
        ScrapeHandlerBuilder::new()
    }

    /// Encodes `registry` in the format the client prefers, see [`negotiate`].
    ///
    /// `header` looks up a request header by its lowercase name.
    ///
    /// # Errors
    ///
    /// Returns a [`ScrapeError`] if encoding the metrics fails.
    pub fn handle<'h>(
        &self,
        registry: &Registry,
        header: impl Fn(&str) -> Option<&'h str>,
    ) -> Result<ScrapeResponse, ScrapeError> {
        let media_type = negotiate(header("accept"), &self.offered);
        let mut body = Vec::new();

        match media_type.encode(&mut body, registry) {
            Ok(()) => Ok(ScrapeResponse {
                status: 200,
                headers: [
                    ("content-type", media_type.content_type()),
                    ("vary", "Accept"),
                ],
                body,
            }),
            Err(err) => Err(ScrapeError {
                status: 500,
                source: err,
            }),
        }
    }
}

const DEFAULT_FORMATS: &[Format] = &[
    Format::PrometheusText0_0_4,
    Format::OpenMetricsText1_0_0,
    #[cfg(feature = "prometheus_protobuf")]
    Format::PrometheusProtobuf,
];

/// A builder for a [`ScrapeHandler`], see [`ScrapeHandler::builder`].
#[derive(Clone, Debug)]
pub struct ScrapeHandlerBuilder {
    offered: Vec<Format>,
}

impl Default for ScrapeHandlerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ScrapeHandlerBuilder {
    /// A builder that offers the Prometheus text format (0.0.4), the OpenMetrics
    /// text format (1.0.0) and, with the `prometheus_protobuf` feature, the
    /// Prometheus protobuf format.
    ///
    /// Formats added in later versions are not offered by default. Use
    /// [`ScrapeHandlerBuilder::with_formats`] to offer them.
    pub fn new() -> Self {
        Self {
            offered: DEFAULT_FORMATS.to_vec(),
        }
    }

    /// The formats to offer, see [`negotiate`] for how one is picked.
    ///
    /// The order is not a preference, since the client's `Accept` header decides.
    /// It only matters when nothing matches and the Prometheus text format is not
    /// offered: then the first format is used.
    ///
    /// An empty list offers the default formats, see [`ScrapeHandlerBuilder::new`].
    pub fn with_formats(mut self, formats: &[Format]) -> Self {
        self.offered = formats.to_vec();
        self
    }

    /// Builds the [`ScrapeHandler`].
    pub fn build(self) -> ScrapeHandler {
        ScrapeHandler {
            offered: if self.offered.is_empty() {
                DEFAULT_FORMATS.to_vec()
            } else {
                self.offered
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "prometheus_protobuf")]
    use crate::encoding::prometheus_protobuf;
    use crate::encoding::{prometheus_text, text, EncodeLabelSet, LabelSetEncoder};
    use crate::metrics::counter::Counter;
    use crate::metrics::family::Family;

    #[test]
    fn handles_scrapes() {
        let mut registry = Registry::default();
        let counter: Counter = Counter::default();
        registry.register("my_counter", "My counter", counter.clone());
        counter.inc();

        let mut prometheus = String::new();
        prometheus_text::encode(&mut prometheus, &registry).unwrap();
        let mut openmetrics = String::new();
        text::encode(&mut openmetrics, &registry).unwrap();
        #[cfg(feature = "prometheus_protobuf")]
        let protobuf = prometheus_protobuf::encode_to_vec(&registry).unwrap();

        let cases: &[(Option<&[Format]>, _, _, &[u8])] = &[
            (
                None,
                None,
                prometheus_text::CONTENT_TYPE,
                prometheus.as_bytes(),
            ),
            (
                None,
                Some("*/*"),
                prometheus_text::CONTENT_TYPE,
                prometheus.as_bytes(),
            ),
            (
                None,
                Some("text/plain;version=0.0.4"),
                prometheus_text::CONTENT_TYPE,
                prometheus.as_bytes(),
            ),
            (
                None,
                Some("application/openmetrics-text;version=1.0.0"),
                text::CONTENT_TYPE,
                openmetrics.as_bytes(),
            ),
            (
                None,
                Some(concat!(
                    "application/openmetrics-text;version=1.0.0;escaping=allow-utf-8;q=0.8,",
                    "application/openmetrics-text;version=0.0.1;q=0.7,",
                    "text/plain;version=1.0.0;escaping=allow-utf-8;q=0.6,",
                    "text/plain;version=0.0.4;q=0.5,*/*;q=0.4",
                )),
                text::CONTENT_TYPE,
                openmetrics.as_bytes(),
            ),
            (
                None,
                Some("application/openmetrics-text;version=1.0.0;q=0.5,text/plain;version=0.0.4"),
                prometheus_text::CONTENT_TYPE,
                prometheus.as_bytes(),
            ),
            #[cfg(feature = "prometheus_protobuf")]
            (
                None,
                Some("application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily;encoding=delimited"),
                prometheus_protobuf::CONTENT_TYPE,
                protobuf.as_slice(),
            ),
            (
                Some(&[]),
                Some("application/openmetrics-text;version=1.0.0"),
                text::CONTENT_TYPE,
                openmetrics.as_bytes(),
            ),
            (
                Some(&[Format::OpenMetricsText1_0_0]),
                None,
                text::CONTENT_TYPE,
                openmetrics.as_bytes(),
            ),
            (
                Some(&[Format::OpenMetricsText1_0_0]),
                Some("text/plain"),
                text::CONTENT_TYPE,
                openmetrics.as_bytes(),
            ),
            (
                Some(&[Format::OpenMetricsText1_0_0]),
                Some("text/plain;version=0.0.4,application/openmetrics-text;version=1.0.0;q=0.1"),
                text::CONTENT_TYPE,
                openmetrics.as_bytes(),
            ),
        ];

        for (formats, accept, content_type, expected) in cases {
            let mut builder = ScrapeHandler::builder();
            if let Some(formats) = formats {
                builder = builder.with_formats(formats);
            }
            let handler = builder.build();

            let response = handler
                .handle(
                    &registry,
                    |name| if name == "accept" { *accept } else { None },
                )
                .unwrap();

            assert_eq!(response.status(), 200, "{formats:?} {accept:?}");
            assert_eq!(
                response.headers().collect::<Vec<_>>(),
                [("content-type", *content_type), ("vary", "Accept")],
                "{formats:?} {accept:?}"
            );
            assert_eq!(response.into_body(), *expected, "{formats:?} {accept:?}");
        }
    }

    #[test]
    fn handle_fails_on_encode_error() {
        #[derive(Clone, Debug, Hash, PartialEq, Eq)]
        struct FailingLabels;

        impl EncodeLabelSet for FailingLabels {
            fn encode(&self, _: &mut LabelSetEncoder) -> Result<(), fmt::Error> {
                Err(fmt::Error)
            }
        }

        let mut registry = Registry::default();
        let family = Family::<FailingLabels, Counter>::default();
        registry.register("my_counter", "My counter", family.clone());
        family.get_or_create(&FailingLabels).inc();

        let accepts = [
            "text/plain;version=0.0.4",
            "application/openmetrics-text;version=1.0.0",
            #[cfg(feature = "prometheus_protobuf")]
            "application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily;encoding=delimited",
        ];

        let handler = ScrapeHandler::default();
        for accept in accepts {
            let err = handler
                .handle(&registry, |name| (name == "accept").then_some(accept))
                .unwrap_err();

            assert_eq!(err.status(), 500, "{accept}");
            let source = std::error::Error::source(&err)
                .and_then(|source| source.downcast_ref::<EncodeError>());
            assert!(
                matches!(source, Some(EncodeError::Fmt(_))),
                "{accept}: {err:?}"
            );
        }
    }
}
