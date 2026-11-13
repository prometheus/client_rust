//! Choosing the exposition format of a response from the `Accept` header of the
//! request.

use crate::encoding::{prometheus_text, text};

#[cfg(feature = "prometheus_protobuf")]
use crate::encoding::prometheus_protobuf;

/// An exposition format that a registry can be encoded in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Format {
    /// The Prometheus text format (version 0.0.4), see [`prometheus_text`].
    PrometheusText0_0_4,
    /// The OpenMetrics text format, see [`text`].
    OpenMetricsText1_0_0,
    /// The Prometheus protobuf format, see [`prometheus_protobuf`].
    #[cfg(feature = "prometheus_protobuf")]
    #[cfg_attr(docsrs, doc(cfg(feature = "prometheus_protobuf")))]
    PrometheusProtobuf,
}

impl Format {
    /// The type and subtype of the media type of this format.
    fn media_type(self) -> (&'static str, &'static str) {
        match self {
            Format::PrometheusText0_0_4 => ("text", "plain"),
            Format::OpenMetricsText1_0_0 => ("application", "openmetrics-text"),
            #[cfg(feature = "prometheus_protobuf")]
            Format::PrometheusProtobuf => ("application", "vnd.google.protobuf"),
        }
    }

    /// The parameters that a media range has to carry to match this format,
    /// with the values that are accepted for each.
    ///
    /// `charset` is left out, as Go's [`matchFormat`](https://github.com/prometheus/common/blob/f072bc2e5c1d524da47e4f7aec401017ab24dce1/expfmt/encode.go#L198-L201)
    /// skips it.
    fn params(self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            Format::PrometheusText0_0_4 => &[("version", &["0.0.4"])],
            // A client that asks for 0.0.1 is answered with 1.0.0.
            Format::OpenMetricsText1_0_0 => &[("version", &["1.0.0", "0.0.1"])],
            #[cfg(feature = "prometheus_protobuf")]
            Format::PrometheusProtobuf => &[
                ("proto", &["io.prometheus.client.MetricFamily"]),
                ("encoding", &["delimited"]),
            ],
        }
    }
}

/// The media type to respond with, as chosen by [`negotiate`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MediaType {
    format: Format,
}

impl MediaType {
    pub(crate) const fn new(format: Format) -> Self {
        Self { format }
    }

    /// The format to encode the response in.
    pub const fn format(&self) -> Format {
        self.format
    }

    /// The `Content-Type` to answer with.
    pub fn content_type(&self) -> &'static str {
        match self.format {
            Format::PrometheusText0_0_4 => prometheus_text::CONTENT_TYPE,
            Format::OpenMetricsText1_0_0 => text::CONTENT_TYPE,
            #[cfg(feature = "prometheus_protobuf")]
            Format::PrometheusProtobuf => prometheus_protobuf::CONTENT_TYPE,
        }
    }
}

/// Picks the media type that a client wants most, from the value of its
/// `Accept` header.
///
/// `offered` are the formats that the caller can answer with, most preferred
/// first.
///
/// This follows [`expfmt.NegotiateAccept`](https://github.com/prometheus/common/blob/f072bc2e5c1d524da47e4f7aec401017ab24dce1/expfmt/encode.go#L129-L154)
/// in Go's `prometheus/common`:
///
/// - The media types in `accept` are tried in the order the client prefers
///   them, and the first offered format that matches one of them is returned.
/// - If nothing matches, or `accept` is `None` or empty, the [`MediaType`] has
///   the format [`Format::PrometheusText0_0_4`] if it is offered, wherever it is
///   in `offered`. Otherwise it has the first of `offered`.
/// - When `offered` is empty, the [`MediaType`] has the format
///   [`Format::PrometheusText0_0_4`].
///
/// # Example
///
/// ```
/// use prometheus_client::encoding::negotiate::{negotiate, Format};
/// use prometheus_client::encoding::{prometheus_text, text};
/// # #[cfg(feature = "prometheus_protobuf")]
/// use prometheus_client::encoding::prometheus_protobuf;
/// use prometheus_client::metrics::counter::Counter;
/// use prometheus_client::registry::Registry;
///
/// let mut registry = Registry::default();
/// let counter: Counter = Counter::default();
/// registry.register("my_counter", "This is my counter", counter.clone());
///
/// // The value of the `Accept` header of the request.
/// let accept = Some("text/plain;version=0.0.4;q=0.5,application/openmetrics-text;version=1.0.0");
///
/// let offered = [
///     Format::PrometheusText0_0_4,
///     Format::OpenMetricsText1_0_0,
///     #[cfg(feature = "prometheus_protobuf")]
///     Format::PrometheusProtobuf,
/// ];
/// let media_type = negotiate(accept, &offered);
///
/// let body: Vec<u8> = match media_type.format() {
///     Format::PrometheusText0_0_4 => {
///         let mut body = String::new();
///         prometheus_text::encode(&mut body, &registry).unwrap();
///         body.into_bytes()
///     }
///     Format::OpenMetricsText1_0_0 => {
///         let mut body = String::new();
///         text::encode(&mut body, &registry).unwrap();
///         body.into_bytes()
///     }
///     // The wire format is `MetricFamily` messages back to back, each with
///     // a length prefix, which `encode_to_vec` writes.
///     #[cfg(feature = "prometheus_protobuf")]
///     Format::PrometheusProtobuf => prometheus_protobuf::encode_to_vec(&registry).unwrap(),
///     // `Format` is non-exhaustive, and only offered formats are returned.
///     _ => unreachable!(),
/// };
///
/// // Respond with `body`, and `media_type.content_type()` as the `Content-Type`.
/// assert_eq!(media_type.format(), Format::OpenMetricsText1_0_0);
/// assert_eq!(
///     media_type.content_type(),
///     "application/openmetrics-text; version=1.0.0; charset=utf-8"
/// );
/// assert!(!body.is_empty());
/// ```
pub fn negotiate(accept: Option<&str>, offered: impl AsRef<[Format]>) -> MediaType {
    let offered = offered.as_ref();
    if offered.is_empty() {
        return MediaType::new(Format::PrometheusText0_0_4);
    }

    let Some(accept) = accept else {
        return fallback(offered);
    };

    // The media range that the client wants most, which is the one with the
    // highest weight, then the most specific, then the first in the header.
    let mut best: Option<(Format, f32, u8)> = None;
    for range in parse_accept(accept) {
        let Some(format) = offered.iter().copied().find(|f| range.matches(*f)) else {
            continue;
        };

        let specificity = range.specificity();

        if best.is_none_or(|(_, q, s)| (range.q, specificity) > (q, s)) {
            best = Some((format, range.q, specificity));
        }
    }

    best.map_or_else(|| fallback(offered), |(format, ..)| MediaType::new(format))
}

/// The text format if it is offered and otherwise the first of `offered`.
fn fallback(offered: &[Format]) -> MediaType {
    if offered.contains(&Format::PrometheusText0_0_4) {
        return MediaType::new(Format::PrometheusText0_0_4);
    }
    MediaType::new(offered[0])
}

/// A media range from an `Accept` header, with its weight and parameters.
#[derive(Debug)]
struct MediaRange<'a> {
    type_: &'a str,
    subtype: &'a str,
    q: f32,
    /// Everything after the media type, unparsed.
    params: &'a str,
}

impl<'a> MediaRange<'a> {
    /// Whether this range accepts `format`, where `*` accepts any type or
    /// subtype and the range has to carry the parameters of the format.
    fn matches(&self, format: Format) -> bool {
        let (type_, subtype) = format.media_type();
        let matches_type = self.type_.eq_ignore_ascii_case(type_);
        let matches_subtype = self.subtype.eq_ignore_ascii_case(subtype);
        if !(matches_type || self.type_ == "*") || !(matches_subtype || self.subtype == "*") {
            return false;
        }

        format
            .params()
            .iter()
            .all(|(key, values)| match self.param(key) {
                Some(v) => values.contains(&v),
                // Only `version` is optional, and only when the range names the
                // media type. A wildcard like `*/*` has no default version and
                // matches nothing, so a client that accepts anything gets the
                // fallback and not the first offered format.
                None => *key == "version" && matches_type && matches_subtype,
            })
    }

    fn param(&self, key: &str) -> Option<&'a str> {
        self.params
            .split(';')
            .filter_map(split_param)
            .find_map(|(k, v)| k.eq_ignore_ascii_case(key).then_some(v))
    }

    /// `*/*` is the least specific, then `type/*`, then `type/subtype`.
    fn specificity(&self) -> u8 {
        match (self.type_, self.subtype) {
            ("*", _) => 0,
            (_, "*") => 1,
            _ => 2,
        }
    }
}

/// Parses the value of an `Accept` header, a list of media ranges, skipping
/// entries that are not valid. See [RFC 9110 section 12.5.1](https://www.rfc-editor.org/rfc/rfc9110#section-12.5.1).
fn parse_accept(accept: &str) -> impl Iterator<Item = MediaRange<'_>> {
    accept.split(',').filter_map(parse_media_range)
}

fn parse_media_range(entry: &str) -> Option<MediaRange<'_>> {
    let (media_type, params) = entry.split_once(';').unwrap_or((entry, ""));
    let media_type = media_type.trim();
    let (type_, subtype) = match media_type.split_once('/') {
        Some((type_, subtype)) if !subtype.contains('/') => (type_.trim(), subtype.trim()),
        None if media_type == "*" => ("*", "*"),
        _ => return None,
    };

    // Only `q` is needed to rank ranges. The other params are read on demand
    // by `MediaRange::param`, which avoids allocating a Vec for them.
    let q = params
        .split(';')
        .filter_map(split_param)
        .rfind(|(k, _)| k.eq_ignore_ascii_case("q"))
        .map_or(1.0, |(_, v)| v.parse().unwrap_or(0.0));

    Some(MediaRange {
        type_,
        subtype,
        q,
        params,
    })
}

/// Splits a `key=value` parameter, skipping malformed ones.
fn split_param(param: &str) -> Option<(&str, &str)> {
    let (key, value) = param.split_once('=')?;
    if value.contains('=') {
        return None;
    }
    Some((key.trim(), value.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back() {
        let cases: &[(Option<&str>, &[Format], Format)] = &[
            // Nothing offered: the text format, as Go's NegotiateAccept does
            // with `FmtText` when `accepted` is empty:
            // https://github.com/prometheus/common/blob/f072bc2e5c1d524da47e4f7aec401017ab24dce1/expfmt/encode.go#L150-L153
            (None, &[], Format::PrometheusText0_0_4),
            (Some(""), &[], Format::PrometheusText0_0_4),
            (Some("*/*"), &[], Format::PrometheusText0_0_4),
            (
                Some("application/openmetrics-text"),
                &[],
                Format::PrometheusText0_0_4,
            ),
            // The text format is offered: it is preferred wherever it is, as
            // in Go:
            // https://github.com/prometheus/common/blob/f072bc2e5c1d524da47e4f7aec401017ab24dce1/expfmt/encode.go#L145-L149
            (
                None,
                &[Format::OpenMetricsText1_0_0, Format::PrometheusText0_0_4],
                Format::PrometheusText0_0_4,
            ),
            (
                None,
                &[Format::PrometheusText0_0_4, Format::OpenMetricsText1_0_0],
                Format::PrometheusText0_0_4,
            ),
            (
                Some(""),
                &[Format::OpenMetricsText1_0_0, Format::PrometheusText0_0_4],
                Format::PrometheusText0_0_4,
            ),
            (
                Some("application/json"),
                &[Format::OpenMetricsText1_0_0, Format::PrometheusText0_0_4],
                Format::PrometheusText0_0_4,
            ),
            // The text format is not offered: the first offered format, as Go
            // does with `accepted[0]`:
            // https://github.com/prometheus/common/blob/f072bc2e5c1d524da47e4f7aec401017ab24dce1/expfmt/encode.go#L150-L152
            (
                None,
                &[Format::OpenMetricsText1_0_0],
                Format::OpenMetricsText1_0_0,
            ),
            (
                Some(""),
                &[Format::OpenMetricsText1_0_0],
                Format::OpenMetricsText1_0_0,
            ),
            (
                Some("application/json"),
                &[Format::OpenMetricsText1_0_0],
                Format::OpenMetricsText1_0_0,
            ),
            (
                Some("text/plain"),
                &[Format::OpenMetricsText1_0_0],
                Format::OpenMetricsText1_0_0,
            ),
        ];

        for (accept, offered, expected) in cases {
            assert_eq!(
                negotiate(*accept, offered).format(),
                *expected,
                "{accept:?}"
            );
        }
    }

    #[test]
    fn negotiates_formats() {
        let offered = [Format::OpenMetricsText1_0_0, Format::PrometheusText0_0_4];

        let cases = [
            ("application/openmetrics-text", Format::OpenMetricsText1_0_0),
            (
                "application/openmetrics-text;version=1.0.0",
                Format::OpenMetricsText1_0_0,
            ),
            (
                "application/openmetrics-text;version=1.0.0;charset=utf-8",
                Format::OpenMetricsText1_0_0,
            ),
            (
                "application/openmetrics-text;version=0.0.1",
                Format::OpenMetricsText1_0_0,
            ),
            (
                "*/*;q=0.1,application/openmetrics-text;version=1.0.0",
                Format::OpenMetricsText1_0_0,
            ),
            // Wildcards have no default version, so they match nothing and
            // fall back to the text format.
            ("application/*", Format::PrometheusText0_0_4),
            ("*/*", Format::PrometheusText0_0_4),
            ("text/*", Format::PrometheusText0_0_4),
            // Wrong versions do not match, so these fall back to the text format.
            (
                "application/openmetrics-text;version=2.0.0",
                Format::PrometheusText0_0_4,
            ),
            ("text/plain", Format::PrometheusText0_0_4),
            ("text/plain;version=0.0.4", Format::PrometheusText0_0_4),
            (
                "text/plain;version=0.0.4;charset=utf-8",
                Format::PrometheusText0_0_4,
            ),
            ("text/plain;version=1.0.0", Format::PrometheusText0_0_4),
            (
                "text/plain;q=0.9,application/openmetrics-text;q=0.1",
                Format::PrometheusText0_0_4,
            ),
        ];

        for (accept, expected) in cases {
            assert_eq!(
                negotiate(Some(accept), offered).format(),
                expected,
                "{accept}"
            );
        }
    }

    #[test]
    fn ranks_matching_ranges() {
        let offered = [Format::OpenMetricsText1_0_0, Format::PrometheusText0_0_4];

        // A wildcard only matches a format when it carries the version of it.
        let cases = [
            // Equal weight and specificity: the first range in the header wins.
            (
                "text/plain, application/openmetrics-text",
                Format::PrometheusText0_0_4,
            ),
            (
                "application/openmetrics-text, text/plain",
                Format::OpenMetricsText1_0_0,
            ),
            (
                "text/plain;q=0.5, application/openmetrics-text;q=0.5",
                Format::PrometheusText0_0_4,
            ),
            // Equal weight: the more specific range wins, wherever it is.
            ("*/*;version=1.0.0, text/plain", Format::PrometheusText0_0_4),
            ("text/plain, */*;version=1.0.0", Format::PrometheusText0_0_4),
            (
                "*/*;version=0.0.4, application/openmetrics-text",
                Format::OpenMetricsText1_0_0,
            ),
            (
                "application/*;version=1.0.0;q=0.5, text/plain;q=0.5",
                Format::PrometheusText0_0_4,
            ),
            // A higher weight wins over a more specific range.
            (
                "*/*;version=1.0.0, text/plain;q=0.5",
                Format::OpenMetricsText1_0_0,
            ),
        ];

        for (accept, expected) in cases {
            assert_eq!(
                negotiate(Some(accept), offered).format(),
                expected,
                "{accept}"
            );
        }
    }

    #[cfg(feature = "prometheus_protobuf")]
    #[test]
    fn negotiates_protobuf() {
        let offered = [Format::PrometheusProtobuf, Format::PrometheusText0_0_4];

        let cases = [
            (
                "application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily;encoding=delimited",
                Format::PrometheusProtobuf,
            ),
            (
                "application/vnd.google.protobuf;encoding=delimited;proto=io.prometheus.client.MetricFamily",
                Format::PrometheusProtobuf,
            ),
            (
                "application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily;encoding=delimited;q=0.5,text/plain;q=0.1",
                Format::PrometheusProtobuf,
            ),
            // Both `proto` and `encoding` are required and have to match, so these fall back to the text format.
            ("application/vnd.google.protobuf", Format::PrometheusText0_0_4),
            (
                "application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily",
                Format::PrometheusText0_0_4,
            ),
            (
                "application/vnd.google.protobuf;encoding=delimited",
                Format::PrometheusText0_0_4,
            ),
            (
                "application/vnd.google.protobuf;proto=other.Message;encoding=delimited",
                Format::PrometheusText0_0_4,
            ),
            (
                "application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily;encoding=text",
                Format::PrometheusText0_0_4,
            ),
        ];

        for (accept, expected) in cases {
            assert_eq!(
                negotiate(Some(accept), offered).format(),
                expected,
                "{accept}"
            );
        }
    }

    #[test]
    fn parses_accept() {
        // The expected media type, `q` and the params to look up by key.
        type Expected<'a> = (&'a str, &'a str, f32, &'a [(&'a str, Option<&'a str>)]);

        let cases: [(&str, Vec<Expected>); 8] = [
            ("text/plain", vec![("text", "plain", 1.0, &[])]),
            (
                "application/openmetrics-text; version=1.0.0; q=0.8",
                vec![(
                    "application",
                    "openmetrics-text",
                    0.8,
                    &[("version", Some("1.0.0"))],
                )],
            ),
            (
                "application/openmetrics-text;version=1.0.0,\
                text/plain;version=0.0.4;q=0.5,*/*;q=0.1",
                vec![
                    (
                        "application",
                        "openmetrics-text",
                        1.0,
                        &[("version", Some("1.0.0"))],
                    ),
                    ("text", "plain", 0.5, &[("version", Some("0.0.4"))]),
                    ("*", "*", 0.1, &[]),
                ],
            ),
            ("*", vec![("*", "*", 1.0, &[])]),
            ("", vec![]),
            (
                "text, a/b/c, , text/plain",
                vec![("text", "plain", 1.0, &[])],
            ),
            ("text/plain;q=abc", vec![("text", "plain", 0.0, &[])]),
            (
                "text/plain;foo;a=b=c;version=0.0.4",
                vec![(
                    "text",
                    "plain",
                    1.0,
                    &[("foo", None), ("a", None), ("version", Some("0.0.4"))],
                )],
            ),
        ];

        for (accept, expected) in cases {
            let parsed: Vec<_> = parse_accept(accept).collect();
            assert_eq!(parsed.len(), expected.len(), "{accept}");

            for (range, (type_, subtype, q, params)) in parsed.iter().zip(expected) {
                assert_eq!(
                    (range.type_, range.subtype, range.q),
                    (type_, subtype, q),
                    "{accept}"
                );
                for (key, value) in params {
                    assert_eq!(range.param(key), *value, "{accept}: {key}");
                }
            }
        }
    }

    #[test]
    fn matches_formats() {
        let cases: &[(&str, Format, bool)] = &[
            // OpenMetricsText1_0_0
            (
                "application/openmetrics-text",
                Format::OpenMetricsText1_0_0,
                true,
            ),
            (
                "application/openmetrics-text;version=1.0.0",
                Format::OpenMetricsText1_0_0,
                true,
            ),
            (
                "application/openmetrics-text;version=1.0.0;charset=utf-8",
                Format::OpenMetricsText1_0_0,
                true,
            ),
            (
                "application/openmetrics-text;version=0.0.1",
                Format::OpenMetricsText1_0_0,
                true,
            ),
            (
                "application/openmetrics-text;version=2.0.0",
                Format::OpenMetricsText1_0_0,
                false,
            ),
            ("application/*", Format::OpenMetricsText1_0_0, false),
            ("*/*", Format::OpenMetricsText1_0_0, false),
            ("text/plain", Format::OpenMetricsText1_0_0, false),
            // PrometheusText0_0_4
            ("text/plain", Format::PrometheusText0_0_4, true),
            ("text/plain;version=0.0.4", Format::PrometheusText0_0_4, true),
            ("text/plain;version=1.0.0", Format::PrometheusText0_0_4, false),
            ("text/*", Format::PrometheusText0_0_4, false),
            ("*/*", Format::PrometheusText0_0_4, false),
            (
                "application/openmetrics-text",
                Format::PrometheusText0_0_4,
                false,
            ),
            // PrometheusProtobuf
            #[cfg(feature = "prometheus_protobuf")]
            (
                "application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily;encoding=delimited",
                Format::PrometheusProtobuf,
                true,
            ),
            #[cfg(feature = "prometheus_protobuf")]
            (
                "application/vnd.google.protobuf",
                Format::PrometheusProtobuf,
                false,
            ),
            #[cfg(feature = "prometheus_protobuf")]
            (
                "application/vnd.google.protobuf;proto=other.Message;encoding=delimited",
                Format::PrometheusProtobuf,
                false,
            ),
        ];

        for (range, format, expected) in cases {
            let range = parse_media_range(range).unwrap();
            assert_eq!(range.matches(*format), *expected, "{range:?} vs {format:?}");
        }
    }
}
