//! Open Metrics text format implementation.
//!
//! ```
//! # use prometheus_client::encoding::text::{encode, encode_registry, encode_eof};
//! # use prometheus_client::metrics::counter::Counter;
//! # use prometheus_client::registry::Registry;
//! #
//! # // Create registry and counter and register the latter with the former.
//! # let mut registry = Registry::default();
//! # let counter: Counter = Counter::default();
//! # registry.register(
//! #   "my_counter",
//! #   "This is my counter",
//! #   counter.clone(),
//! # );
//! # counter.inc();
//! let mut buffer = String::new();
//!
//! // Encode the complete OpenMetrics exposition into the message buffer
//! encode(&mut buffer, &registry).unwrap();
//! let expected_msg = "# HELP my_counter This is my counter.\n".to_owned() +
//!                    "# TYPE my_counter counter\n" +
//!                    "my_counter_total 1\n" +
//!                    "# EOF\n";
//! assert_eq!(expected_msg, buffer);
//! buffer.clear();
//!
//! // Encode just the registry into the message buffer
//! encode_registry(&mut buffer, &registry).unwrap();
//! let expected_reg = "# HELP my_counter This is my counter.\n".to_owned() +
//!                    "# TYPE my_counter counter\n" +
//!                    "my_counter_total 1\n";
//! assert_eq!(expected_reg, buffer);
//!
//! // Encode EOF marker into message buffer to complete the OpenMetrics exposition
//! encode_eof(&mut buffer).unwrap();
//! assert_eq!(expected_msg, buffer);
//! ```

use crate::encoding::scratch::{Offset, ScratchBuf, Span};
use crate::encoding::{
    EncodeExemplarTime, EncodeExemplarValue, EncodeLabelSet, NativeHistogram, NoLabelSet,
};
use crate::metrics::exemplar::Exemplar;
use crate::metrics::MetricType;
use crate::registry::{Prefix, Registry, Unit};

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write;

/// Encode both the metrics registered with the provided [`Registry`] and the
/// EOF marker into the provided [`Write`]r using the OpenMetrics text format.
///
/// Note: This function encodes the **complete** OpenMetrics exposition.
///
/// Use [`encode_registry`] or [`encode_eof`] if partial encoding is needed.
///
/// See [OpenMetrics exposition format](https://github.com/prometheus/OpenMetrics/blob/v1.0.0/specification/OpenMetrics.md#text-format)
/// for additional details.
///
/// # Examples
///
/// ```no_run
/// # use prometheus_client::encoding::text::encode;
/// # use prometheus_client::metrics::counter::Counter;
/// # use prometheus_client::metrics::gauge::Gauge;
/// # use prometheus_client::registry::Registry;
/// #
/// // Initialize registry with metric families
/// let mut registry = Registry::default();
/// let counter: Counter = Counter::default();
/// registry.register(
///     "my_counter",
///     "This is my counter",
///     counter.clone(),
/// );
/// let gauge: Gauge = Gauge::default();
/// registry.register(
///     "my_gauge",
///     "This is my gauge",
///     gauge.clone(),
/// );
///
/// // Encode the complete OpenMetrics exposition into the buffer
/// let mut buffer = String::new();
/// encode(&mut buffer, &registry)?;
/// # Ok::<(), std::fmt::Error>(())
/// ```
pub fn encode<W>(writer: &mut W, registry: &Registry) -> Result<(), std::fmt::Error>
where
    W: Write,
{
    encode_registry(writer, registry)?;
    encode_eof(writer)
}

/// Encode the metrics registered with the provided [`Registry`] into the
/// provided [`Write`]r using the OpenMetrics text format.
///
/// Note: The OpenMetrics exposition requires that a complete message must end
/// with an EOF marker.
///
/// This function may be called repeatedly for the HTTP scrape response until
/// [`encode_eof`] signals the end of the response.
///
/// This may also be used to compose a partial message with metrics assembled
/// from multiple registries.
///
/// # Examples
///
/// ```no_run
/// # use prometheus_client::encoding::text::encode_registry;
/// # use prometheus_client::metrics::counter::Counter;
/// # use prometheus_client::metrics::gauge::Gauge;
/// # use prometheus_client::registry::Registry;
/// #
/// // Initialize registry with a counter
/// let mut reg_counter = Registry::default();
/// let counter: Counter = Counter::default();
/// reg_counter.register(
///     "my_counter",
///     "This is my counter",
///     counter.clone(),
/// );
///
/// // Encode the counter registry into the buffer
/// let mut buffer = String::new();
/// encode_registry(&mut buffer, &reg_counter)?;
///
/// // Initialize another registry but with a gauge
/// let mut reg_gauge = Registry::default();
/// let gauge: Gauge = Gauge::default();
/// reg_gauge.register(
///   "my_gauge",
///   "This is my gauge",
///   gauge.clone(),
/// );
///
/// // Encode the gauge registry into the buffer
/// encode_registry(&mut buffer, &reg_gauge)?;
/// # Ok::<(), std::fmt::Error>(())
/// ```
pub fn encode_registry<W>(writer: &mut W, registry: &Registry) -> Result<(), std::fmt::Error>
where
    W: Write,
{
    let mut buffer = TextBuf::new_buffer();
    let mut scratch = ScratchBuf::new();
    registry.encode(
        &mut DescriptorEncoder::new(TextBuf::new(&mut buffer, writer), &mut scratch).into(),
    )?;
    writer.write_str(&buffer)
}

/// Size (in bytes) at which [`TextBuf`] flushes its buffer to the sink.
///
/// Flushed chunks are at least this large, except for the final remainder and
/// for a flush that precedes a write larger than this threshold (such writes
/// bypass the buffer, see [`TextBuf::write_large`]).
pub(crate) const FLUSH_THRESHOLD: usize = 4 * 1024;

/// Largest size the buffer can reach: it holds at most `FLUSH_THRESHOLD - 1`
/// bytes before a write, and a single buffered write is at most
/// `FLUSH_THRESHOLD` bytes. Reserving this up front keeps
/// [`TextBuf::write_str`] allocation free.
const BUFFER_CAPACITY: usize = FLUSH_THRESHOLD * 2 - 1;

/// Buffered writer used by all text encoders.
pub(crate) struct TextBuf<'a> {
    buf: &'a mut String,
    sink: &'a mut dyn Write,
}

impl<'a> TextBuf<'a> {
    pub(crate) fn new(buf: &'a mut String, sink: &'a mut dyn Write) -> Self {
        Self { buf, sink }
    }

    /// Reborrow with a shorter lifetime, e.g. to hand to a child encoder.
    #[inline]
    pub(crate) fn reborrow(&mut self) -> TextBuf<'_> {
        TextBuf {
            buf: self.buf,
            sink: self.sink,
        }
    }

    /// Allocate a buffer large enough that [`TextBuf::write_str`] never has to
    /// grow it.
    pub(crate) fn new_buffer() -> String {
        String::with_capacity(BUFFER_CAPACITY)
    }

    #[cold]
    #[inline(never)]
    fn flush(&mut self) -> std::fmt::Result {
        self.sink.write_str(self.buf)?;
        self.buf.clear();
        Ok(())
    }

    /// Flush pending output, then hand `s` to the sink directly instead of
    /// copying it through (and growing) the buffer.
    #[cold]
    #[inline(never)]
    fn write_large(&mut self, s: &str) -> std::fmt::Result {
        if !self.buf.is_empty() {
            self.flush()?;
        }
        self.sink.write_str(s)
    }
}

impl Write for TextBuf<'_> {
    #[inline]
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if s.len() > FLUSH_THRESHOLD {
            return self.write_large(s);
        }
        self.buf.push_str(s);
        if self.buf.len() >= FLUSH_THRESHOLD {
            self.flush()?;
        }
        Ok(())
    }
}

impl TextBuf<'_> {
    /// Run `f` with a [`TextBuf`] and collect everything it writes into a
    /// `String`.
    pub(crate) fn encode_to_string(
        f: impl FnOnce(TextBuf<'_>) -> std::fmt::Result,
    ) -> Result<String, std::fmt::Error> {
        let mut out = String::new();
        let mut buf = String::new();
        f(TextBuf::new(&mut buf, &mut out))?;
        out.push_str(&buf);
        Ok(out)
    }
}

/// Encode the EOF marker into the provided [`Write`]r using the OpenMetrics
/// text format.
///
/// Note: This function is used to mark/signal the end of the exposition.
///
/// # Examples
///
/// ```no_run
/// # use prometheus_client::encoding::text::{encode_registry, encode_eof};
/// # use prometheus_client::metrics::counter::Counter;
/// # use prometheus_client::metrics::gauge::Gauge;
/// # use prometheus_client::registry::Registry;
/// #
/// // Initialize registry with a counter
/// let mut registry = Registry::default();
/// let counter: Counter = Counter::default();
/// registry.register(
///     "my_counter",
///     "This is my counter",
///     counter.clone(),
/// );
///
/// // Encode registry into the buffer
/// let mut buffer = String::new();
/// encode_registry(&mut buffer, &registry)?;
///
/// // Encode EOF marker to complete the message
/// encode_eof(&mut buffer)?;
/// # Ok::<(), std::fmt::Error>(())
/// ```
pub fn encode_eof<W>(writer: &mut W) -> Result<(), std::fmt::Error>
where
    W: Write,
{
    writer.write_str("# EOF\n")
}

pub(crate) struct DescriptorEncoder<'a> {
    writer: TextBuf<'a>,
    scratch: &'a mut ScratchBuf,
    prefix: Option<&'a Prefix>,
    labels: &'a [(Cow<'static, str>, Cow<'static, str>)],
}

impl std::fmt::Debug for DescriptorEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DescriptorEncoder").finish()
    }
}

impl<'a> DescriptorEncoder<'a> {
    pub(crate) fn new(writer: TextBuf<'a>, scratch: &'a mut ScratchBuf) -> DescriptorEncoder<'a> {
        DescriptorEncoder {
            writer,
            scratch,
            prefix: Default::default(),
            labels: Default::default(),
        }
    }

    pub(crate) fn with_prefix_and_labels<'s>(
        &'s mut self,
        prefix: Option<&'s Prefix>,
        labels: &'s [(Cow<'static, str>, Cow<'static, str>)],
    ) -> DescriptorEncoder<'s> {
        DescriptorEncoder {
            prefix,
            labels,
            writer: self.writer.reborrow(),
            scratch: &mut *self.scratch,
        }
    }

    pub fn encode_descriptor<'s>(
        &'s mut self,
        name: &'s str,
        help: &str,
        unit: Option<&'s Unit>,
        metric_type: MetricType,
    ) -> Result<MetricEncoder<'s>, std::fmt::Error> {
        self.writer.write_str("# HELP ")?;
        if let Some(prefix) = self.prefix {
            self.writer.write_str(prefix.as_str())?;
            self.writer.write_str("_")?;
        }
        self.writer.write_str(name)?;
        if let Some(unit) = unit {
            self.writer.write_str("_")?;
            self.writer.write_str(unit.as_str())?;
        }
        self.writer.write_str(" ")?;
        self.writer.write_str(help)?;
        self.writer.write_str("\n")?;

        self.writer.write_str("# TYPE ")?;
        if let Some(prefix) = self.prefix {
            self.writer.write_str(prefix.as_str())?;
            self.writer.write_str("_")?;
        }
        self.writer.write_str(name)?;
        if let Some(unit) = unit {
            self.writer.write_str("_")?;
            self.writer.write_str(unit.as_str())?;
        }
        self.writer.write_str(" ")?;
        self.writer.write_str(metric_type.as_str())?;
        self.writer.write_str("\n")?;

        if let Some(unit) = unit {
            self.writer.write_str("# UNIT ")?;
            if let Some(prefix) = self.prefix {
                self.writer.write_str(prefix.as_str())?;
                self.writer.write_str("_")?;
            }
            self.writer.write_str(name)?;
            self.writer.write_str("_")?;
            self.writer.write_str(unit.as_str())?;
            self.writer.write_str(" ")?;
            self.writer.write_str(unit.as_str())?;
            self.writer.write_str("\n")?;
        }

        Ok(MetricEncoder {
            writer: self.writer.reborrow(),
            scratch: &mut *self.scratch,
            prefix: self.prefix,
            name,
            unit,
            const_labels: self.labels,
            family_labels: None,
        })
    }
}

/// Helper type for [`EncodeMetric`](super::EncodeMetric), see
/// [`EncodeMetric::encode`](super::EncodeMetric::encode).
///
// `MetricEncoder` does not take a trait parameter for `writer` and `labels`
// because `EncodeMetric` which uses `MetricEncoder` needs to be usable as a
// trait object in order to be able to register different metric types with a
// `Registry`. Trait objects can not use type parameters.
//
// TODO: Alternative solutions to the above are very much appreciated.
pub(crate) struct MetricEncoder<'a> {
    writer: TextBuf<'a>,
    scratch: &'a mut ScratchBuf,
    prefix: Option<&'a Prefix>,
    name: &'a str,
    unit: Option<&'a Unit>,
    const_labels: &'a [(Cow<'static, str>, Cow<'static, str>)],
    family_labels: Option<&'a dyn super::EncodeLabelSet>,
}

impl std::fmt::Debug for MetricEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let labels = TextBuf::encode_to_string(|buf| match self.family_labels {
            Some(l) => LabelList::new("").encode(buf, l),
            None => Ok(()),
        })?;

        f.debug_struct("Encoder")
            .field("name", &self.name)
            .field("prefix", &self.prefix)
            .field("unit", &self.unit)
            .field("const_labels", &self.const_labels)
            .field("labels", &labels.as_str())
            .finish()
    }
}

impl MetricEncoder<'_> {
    pub fn encode_counter<
        S: EncodeLabelSet,
        CounterValue: super::EncodeCounterValue,
        ExemplarValue: EncodeExemplarValue,
    >(
        &mut self,
        v: &CounterValue,
        exemplar: Option<&Exemplar<S, ExemplarValue>>,
    ) -> Result<(), std::fmt::Error> {
        self.write_prefix_name_unit()?;

        self.write_suffix("total")?;

        self.encode_labels::<NoLabelSet>(None)?;

        v.encode(
            &mut CounterValueEncoder {
                writer: self.writer.reborrow(),
            }
            .into(),
        )?;

        if let Some(exemplar) = exemplar {
            self.encode_exemplar(exemplar)?;
        }

        self.newline()?;

        Ok(())
    }

    pub fn encode_gauge<GaugeValue: super::EncodeGaugeValue>(
        &mut self,
        v: &GaugeValue,
    ) -> Result<(), std::fmt::Error> {
        self.write_prefix_name_unit()?;

        self.encode_labels::<NoLabelSet>(None)?;

        v.encode(
            &mut GaugeValueEncoder {
                writer: self.writer.reborrow(),
            }
            .into(),
        )?;

        self.newline()?;

        Ok(())
    }

    pub fn encode_info<S: EncodeLabelSet>(&mut self, label_set: &S) -> Result<(), std::fmt::Error> {
        self.write_prefix_name_unit()?;

        self.write_suffix("info")?;

        self.encode_labels(Some(label_set))?;

        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(1))?;

        self.newline()?;

        Ok(())
    }

    /// Encode a set of labels. Used by wrapper metric types like
    /// [`Family`](crate::metrics::family::Family).
    pub fn encode_family<'s, S: EncodeLabelSet>(
        &'s mut self,
        label_set: &'s S,
    ) -> Result<MetricEncoder<'s>, std::fmt::Error> {
        debug_assert!(self.family_labels.is_none());

        Ok(MetricEncoder {
            writer: self.writer.reborrow(),
            scratch: &mut *self.scratch,
            prefix: self.prefix,
            name: self.name,
            unit: self.unit,
            const_labels: self.const_labels,
            family_labels: Some(label_set),
        })
    }

    pub fn encode_histogram<S: EncodeLabelSet>(
        &mut self,
        sum: f64,
        count: u64,
        buckets: &[(f64, u64)],
        exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
    ) -> Result<(), std::fmt::Error> {
        // Encode the const and family labels once and reuse them for every
        // line.
        let start = self.scratch.offset();
        let result = self
            .encode_cached_labels()
            .and_then(|labels| self.write_histogram_lines(labels, sum, count, buckets, exemplars));
        self.scratch.rewind(start);
        result
    }

    /// Encodes the const and family labels into the scratch buffer as a
    /// `k="v",...` sequence and returns its span.
    fn encode_cached_labels(&mut self) -> Result<Span, std::fmt::Error> {
        let mut seq = LabelSeqEncoder::new(self.scratch);
        seq.encode(&self.const_labels)?;
        if let Some(labels) = self.family_labels {
            seq.encode(labels)?;
        }
        Ok(seq.finish())
    }

    /// Writes the `_sum`, `_count` and bucket lines of a histogram whose
    /// const and family labels are cached in the scratch buffer at `labels`.
    fn write_histogram_lines<S: EncodeLabelSet>(
        &mut self,
        labels: Span,
        sum: f64,
        count: u64,
        buckets: &[(f64, u64)],
        exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
    ) -> Result<(), std::fmt::Error> {
        self.write_prefix_name_unit()?;
        self.write_suffix("sum")?;
        self.write_labels_raw::<NoLabelSet>(labels, None)?;
        self.writer.write_str(" ")?;
        self.writer.write_str(dtoa::Buffer::new().format(sum))?;
        self.newline()?;

        self.write_prefix_name_unit()?;
        self.write_suffix("count")?;
        self.write_labels_raw::<NoLabelSet>(labels, None)?;
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(count))?;
        self.newline()?;

        let mut cummulative = 0;
        for (i, (upper_bound, count)) in buckets.iter().enumerate() {
            cummulative += count;

            self.write_prefix_name_unit()?;
            self.write_suffix("bucket")?;

            if *upper_bound == f64::INFINITY {
                self.write_labels_raw(labels, Some(&[("le", "+Inf")]))?;
            } else {
                self.write_labels_raw(labels, Some(&[("le", *upper_bound)]))?;
            }

            self.writer.write_str(" ")?;
            self.writer
                .write_str(itoa::Buffer::new().format(cummulative))?;

            if let Some(exemplar) = exemplars.and_then(|e| e.get(&i)) {
                self.encode_exemplar(exemplar)?
            }

            self.newline()?;
        }

        Ok(())
    }

    pub fn encode_histogram_with_native<S: EncodeLabelSet>(
        &mut self,
        sum: f64,
        count: u64,
        buckets: &[(f64, u64)],
        exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
        _native: NativeHistogram<'_>,
    ) -> Result<(), std::fmt::Error> {
        if buckets.is_empty() {
            return Err(std::fmt::Error);
        }

        self.encode_histogram(sum, count, buckets, exemplars)
    }

    /// Encode an exemplar for the given metric.
    fn encode_exemplar<S: EncodeLabelSet, V: EncodeExemplarValue>(
        &mut self,
        exemplar: &Exemplar<S, V>,
    ) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" # {")?;
        LabelList::new("").encode(self.writer.reborrow(), &exemplar.label_set)?;
        self.writer.write_str("} ")?;
        exemplar.value.encode(
            ExemplarValueEncoder {
                writer: self.writer.reborrow(),
            }
            .into(),
        )?;
        if let Some(timestamp) = exemplar.timestamp {
            self.writer.write_char(' ')?;
            timestamp.encode(
                ExemplarValueEncoder {
                    writer: self.writer.reborrow(),
                }
                .into(),
            )?;
        }
        Ok(())
    }

    fn newline(&mut self) -> Result<(), std::fmt::Error> {
        self.writer.write_str("\n")
    }
    fn write_prefix_name_unit(&mut self) -> Result<(), std::fmt::Error> {
        if let Some(prefix) = self.prefix {
            self.writer.write_str(prefix.as_str())?;
            self.writer.write_str("_")?;
        }
        self.writer.write_str(self.name)?;
        if let Some(unit) = self.unit {
            self.writer.write_str("_")?;
            self.writer.write_str(unit.as_str())?;
        }

        Ok(())
    }

    fn write_suffix(&mut self, suffix: &'static str) -> Result<(), std::fmt::Error> {
        self.writer.write_str("_")?;
        self.writer.write_str(suffix)?;

        Ok(())
    }

    fn encode_labels<S: EncodeLabelSet>(
        &mut self,
        additional_labels: Option<&S>,
    ) -> Result<(), std::fmt::Error> {
        let mut list = LabelList::new("{");
        list.encode(self.writer.reborrow(), &self.const_labels)?;
        if let Some(labels) = self.family_labels {
            list.encode(self.writer.reborrow(), labels)?;
        }
        if let Some(labels) = additional_labels {
            list.encode(self.writer.reborrow(), labels)?;
        }
        list.finish(self.writer.reborrow(), "}")
    }

    /// Like [`Self::encode_labels`], but with the const and family labels
    /// replaced by the sequence at `cached` in the scratch buffer, pre-encoded
    /// by a [`LabelSeqEncoder`].
    fn write_labels_raw<S: EncodeLabelSet>(
        &mut self,
        cached: Span,
        additional_labels: Option<&S>,
    ) -> Result<(), std::fmt::Error> {
        let cached = self.scratch.get(cached).ok_or(std::fmt::Error)?;
        let mut list = LabelList::new("{");
        list.encode_raw(self.writer.reborrow(), cached)?;
        if let Some(labels) = additional_labels {
            list.encode(self.writer.reborrow(), labels)?;
        }
        list.finish(self.writer.reborrow(), "}")
    }
}

pub(crate) struct CounterValueEncoder<'a> {
    writer: TextBuf<'a>,
}

impl std::fmt::Debug for CounterValueEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CounterValueEncoder").finish()
    }
}

impl CounterValueEncoder<'_> {
    pub fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(dtoa::Buffer::new().format(v))?;
        Ok(())
    }

    pub fn encode_u64(&mut self, v: u64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(v))?;
        Ok(())
    }
}

pub(crate) struct GaugeValueEncoder<'a> {
    writer: TextBuf<'a>,
}

impl std::fmt::Debug for GaugeValueEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GaugeValueEncoder").finish()
    }
}

impl GaugeValueEncoder<'_> {
    pub fn encode_u32(&mut self, v: u32) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(v))?;
        Ok(())
    }

    pub fn encode_u64(&mut self, v: u64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(v))?;
        Ok(())
    }

    pub fn encode_i64(&mut self, v: i64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(v))?;
        Ok(())
    }

    pub fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(dtoa::Buffer::new().format(v))?;
        Ok(())
    }
}

pub(crate) struct ExemplarValueEncoder<'a> {
    writer: TextBuf<'a>,
}

impl std::fmt::Debug for ExemplarValueEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExemplarValueEncoder").finish()
    }
}

impl ExemplarValueEncoder<'_> {
    pub fn encode(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(dtoa::Buffer::new().format(v))
    }
}

/// Joins one or more label sets into a single comma-separated list.
///
/// `open` is written lazily before the first label key, so label sets which
/// turn out to be empty, as well as labels which never encode a key, produce
/// no output (in particular, no stray `,` or `{}`).
#[derive(Debug)]
pub(crate) struct LabelList {
    open: &'static str,
    written: bool,
}

impl LabelList {
    pub(crate) fn new(open: &'static str) -> Self {
        Self {
            open,
            written: false,
        }
    }

    /// Append all labels of `labels` to the list.
    pub(crate) fn encode<S: EncodeLabelSet + ?Sized>(
        &mut self,
        mut writer: TextBuf<'_>,
        labels: &S,
    ) -> Result<(), std::fmt::Error> {
        labels.encode(&mut LabelSetEncoder::new(writer.reborrow(), self).into())
    }

    /// Append `encoded`, a `k="v",...` sequence previously produced by a
    /// `LabelList::new("")`, e.g. via [`LabelSeqEncoder`].
    pub(crate) fn encode_raw(
        &mut self,
        mut writer: TextBuf<'_>,
        encoded: &str,
    ) -> Result<(), std::fmt::Error> {
        if encoded.is_empty() {
            return Ok(());
        }
        let separator = if self.written { "," } else { self.open };
        self.written = true;
        if !separator.is_empty() {
            writer.write_str(separator)?;
        }
        writer.write_str(encoded)
    }

    /// Write `close` if at least one label has been written.
    pub(crate) fn finish(
        self,
        mut writer: TextBuf<'_>,
        close: &str,
    ) -> Result<(), std::fmt::Error> {
        if self.written {
            writer.write_str(close)?;
        }
        Ok(())
    }
}

/// Encodes one or more label sets into a [`ScratchBuf`] as a single
/// `k="v",...` sequence (without braces), so it can be encoded once and then
/// spliced into many label lists via [`LabelList::encode_raw`].
pub(crate) struct LabelSeqEncoder<'a> {
    scratch: &'a mut ScratchBuf,
    list: LabelList,
    start: Offset,
}

impl std::fmt::Debug for LabelSeqEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelSeqEncoder")
            .field("list", &self.list)
            .field("start", &self.start)
            .finish()
    }
}

impl<'a> LabelSeqEncoder<'a> {
    /// Starts a new sequence at the current top of `scratch`.
    pub(crate) fn new(scratch: &'a mut ScratchBuf) -> Self {
        Self {
            start: scratch.offset(),
            scratch,
            list: LabelList::new(""),
        }
    }

    /// Append all labels of `labels` to the sequence.
    pub(crate) fn encode<S: EncodeLabelSet + ?Sized>(
        &mut self,
        labels: &S,
    ) -> Result<(), std::fmt::Error> {
        let list = &mut self.list;
        self.scratch.encode_text(|buf| list.encode(buf, labels))
    }

    /// Returns the span of the encoded sequence within the scratch buffer.
    pub(crate) fn finish(self) -> Span {
        self.scratch.span_since(self.start)
    }
}

pub(crate) struct LabelSetEncoder<'a> {
    writer: TextBuf<'a>,
    list: &'a mut LabelList,
}

impl std::fmt::Debug for LabelSetEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelSetEncoder")
            .field("list", &self.list)
            .finish()
    }
}

impl<'a> LabelSetEncoder<'a> {
    pub(crate) fn new(writer: TextBuf<'a>, list: &'a mut LabelList) -> Self {
        Self { writer, list }
    }

    pub fn encode_label(&mut self) -> LabelEncoder<'_> {
        LabelEncoder {
            writer: self.writer.reborrow(),
            list: &mut *self.list,
        }
    }
}

pub(crate) struct LabelEncoder<'a> {
    writer: TextBuf<'a>,
    list: &'a mut LabelList,
}

impl std::fmt::Debug for LabelEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelEncoder")
            .field("list", &self.list)
            .finish()
    }
}

impl LabelEncoder<'_> {
    pub fn encode_label_key(&mut self) -> Result<LabelKeyEncoder<'_>, std::fmt::Error> {
        // Only mark the list as written once a key is actually encoded, so a
        // label which never writes a key leaves no stray separator behind.
        let separator = if self.list.written {
            ","
        } else {
            self.list.open
        };
        self.list.written = true;
        if !separator.is_empty() {
            self.writer.write_str(separator)?;
        }
        Ok(LabelKeyEncoder {
            writer: self.writer.reborrow(),
        })
    }
}

pub(crate) struct LabelKeyEncoder<'a> {
    writer: TextBuf<'a>,
}

impl std::fmt::Debug for LabelKeyEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelKeyEncoder").finish()
    }
}

impl<'a> LabelKeyEncoder<'a> {
    pub fn encode_label_value(mut self) -> Result<LabelValueEncoder<'a>, std::fmt::Error> {
        self.writer.write_str("=\"")?;
        Ok(LabelValueEncoder {
            writer: self.writer,
        })
    }
}

impl std::fmt::Write for LabelKeyEncoder<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.writer.write_str(s)
    }
}

pub(crate) struct LabelValueEncoder<'a> {
    writer: TextBuf<'a>,
}

impl std::fmt::Debug for LabelValueEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelValueEncoder").finish()
    }
}

impl LabelValueEncoder<'_> {
    pub fn finish(mut self) -> Result<(), std::fmt::Error> {
        self.writer.write_str("\"")
    }

    pub(crate) fn write_str_unescaped(&mut self, s: &str) -> Result<(), std::fmt::Error> {
        self.writer.write_str(s)
    }

    #[cold]
    fn write_str_escaped(&mut self, s: &str) -> Result<(), std::fmt::Error> {
        let mut last = 0;
        for (index, byte) in s.bytes().enumerate() {
            let escaped = match byte {
                b'\\' => "\\\\",
                b'"' => "\\\"",
                b'\n' => "\\n",
                _ => continue,
            };
            self.writer.write_str(&s[last..index])?;
            self.writer.write_str(escaped)?;
            last = index + 1;
        }
        self.writer.write_str(&s[last..])
    }
}

impl std::fmt::Write for LabelValueEncoder<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let needs_escaping = s.bytes().fold(false, |found, byte| {
            found | matches!(byte, b'\\' | b'"' | b'\n')
        });
        if needs_escaping {
            self.write_str_escaped(s)
        } else {
            self.write_str_unescaped(s)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::py_test_support::parse_with_python_client;
    use crate::encoding::py_test_support::Parser;
    use crate::metrics::exemplar::HistogramWithExemplars;
    use crate::metrics::family::Family;
    use crate::metrics::gauge::Gauge;
    use crate::metrics::histogram::{exponential_buckets, Histogram, NativeHistogramConfig};
    use crate::metrics::info::Info;
    use crate::metrics::{counter::Counter, exemplar::CounterWithExemplar};
    use quickcheck::QuickCheck;
    use std::borrow::Cow;
    use std::fmt::Error;
    use std::sync::atomic::{AtomicI32, AtomicU32};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn text_buf_never_reallocates() {
        let mut buf = TextBuf::new_buffer();
        let capacity = buf.capacity();
        let mut sink = String::new();

        let writes = [
            "a".repeat(FLUSH_THRESHOLD - 1),
            // Worst case: `FLUSH_THRESHOLD - 1` bytes pending, then a write of
            // exactly `FLUSH_THRESHOLD` bytes stays on the buffered path.
            "b".repeat(FLUSH_THRESHOLD),
            "c".to_owned(),
            "d".repeat(FLUSH_THRESHOLD - 1),
            // Oversized writes bypass the buffer.
            "e".repeat(3 * FLUSH_THRESHOLD),
            "f".repeat(FLUSH_THRESHOLD - 1),
            "g".repeat(FLUSH_THRESHOLD),
        ];

        let mut text_buf = TextBuf::new(&mut buf, &mut sink);
        for write in &writes {
            text_buf.write_str(write).unwrap();
            assert_eq!(capacity, text_buf.buf.capacity());
        }

        sink.push_str(&buf);
        assert_eq!(writes.concat(), sink);
    }

    const NO_LABELS: [(&str, &str); 0] = [];

    fn encode_list(f: impl FnOnce(&mut LabelList, TextBuf<'_>) -> Result<(), Error>) -> String {
        TextBuf::encode_to_string(|mut buf| {
            let mut list = LabelList::new("{");
            f(&mut list, buf.reborrow())?;
            list.finish(buf, "}")
        })
        .unwrap()
    }

    #[test]
    fn encode_raw_empty_writes_nothing() {
        let encoded = encode_list(|list, buf| list.encode_raw(buf, ""));
        assert_eq!("", encoded);
    }

    #[test]
    fn encode_raw_only() {
        let encoded = encode_list(|list, buf| list.encode_raw(buf, r#"a="1""#));
        assert_eq!(r#"{a="1"}"#, encoded);
    }

    #[test]
    fn encode_raw_after_structured_labels() {
        let encoded = encode_list(|list, mut buf| {
            list.encode(buf.reborrow(), &[("a", "1")])?;
            list.encode_raw(buf, r#"b="2",c="3""#)
        });
        assert_eq!(r#"{a="1",b="2",c="3"}"#, encoded);
    }

    #[test]
    fn encode_raw_before_structured_labels() {
        let encoded = encode_list(|list, mut buf| {
            list.encode_raw(buf.reborrow(), r#"a="1""#)?;
            list.encode(buf, &[("b", "2")])
        });
        assert_eq!(r#"{a="1",b="2"}"#, encoded);
    }

    #[test]
    fn encode_raw_twice() {
        let encoded = encode_list(|list, mut buf| {
            list.encode_raw(buf.reborrow(), r#"a="1""#)?;
            list.encode_raw(buf.reborrow(), "")?;
            list.encode_raw(buf, r#"b="2""#)
        });
        assert_eq!(r#"{a="1",b="2"}"#, encoded);
    }

    #[test]
    fn label_seq_encoder_joins_label_sets() {
        let mut scratch = ScratchBuf::new();
        scratch.push_str("prior");
        let mut seq = LabelSeqEncoder::new(&mut scratch);
        seq.encode(&[("a", "1")]).unwrap();
        seq.encode(&NO_LABELS).unwrap();
        seq.encode(&[("b", "2")]).unwrap();
        let span = seq.finish();
        assert_eq!(Some(r#"a="1",b="2""#), scratch.get(span));
    }

    #[test]
    fn label_seq_encoder_skips_leading_empty_set() {
        let mut scratch = ScratchBuf::new();
        let mut seq = LabelSeqEncoder::new(&mut scratch);
        seq.encode(&NO_LABELS).unwrap();
        seq.encode(&[("a", "1")]).unwrap();
        let span = seq.finish();
        assert_eq!(Some(r#"a="1""#), scratch.get(span));
    }

    #[test]
    fn label_seq_encoder_empty() {
        let mut scratch = ScratchBuf::new();
        let mut seq = LabelSeqEncoder::new(&mut scratch);
        seq.encode(&NO_LABELS).unwrap();
        let span = seq.finish();
        assert_eq!(Some(""), scratch.get(span));
    }

    #[test]
    fn label_seq_encoder_caches_escaped_values() {
        let mut scratch = ScratchBuf::new();
        let mut seq = LabelSeqEncoder::new(&mut scratch);
        seq.encode(&[("a", "x\"\n")]).unwrap();
        let span = seq.finish();
        let cached = scratch.get(span).unwrap().to_owned();
        assert_eq!(r#"a="x\"\n""#, cached);

        let encoded = encode_list(|list, buf| list.encode_raw(buf, &cached));
        assert_eq!(r#"{a="x\"\n"}"#, encoded);
    }

    #[test]
    fn label_values_escape_special_characters() {
        let encoded = TextBuf::encode_to_string(|mut buf| {
            let mut list = LabelList::new("");
            let mut label_set_encoder = LabelSetEncoder::new(buf.reborrow(), &mut list);
            let mut label_encoder = label_set_encoder.encode_label();
            let mut key_encoder = label_encoder.encode_label_key()?;
            key_encoder.write_str("label")?;
            let mut value_encoder = key_encoder.encode_label_value()?;
            value_encoder.write_str("plain \\ quoted \" line\ncarriage\r unicode λ")?;
            value_encoder.finish()
        })
        .unwrap();

        assert_eq!(
            concat!(
                r#"label="plain \\ quoted \" line\ncarriage"#,
                "\r",
                r#" unicode λ""#
            ),
            encoded
        );
    }

    #[test]
    fn label_value_escaping_matches_reference() {
        fn reference(s: &str) -> String {
            let mut escaped = String::new();
            for character in s.chars() {
                match character {
                    '\\' => escaped.push_str("\\\\"),
                    '"' => escaped.push_str("\\\""),
                    '\n' => escaped.push_str("\\n"),
                    _ => escaped.push(character),
                }
            }
            escaped
        }

        fn encode(value: &str) -> String {
            TextBuf::encode_to_string(|writer| LabelValueEncoder { writer }.write_str(value))
                .unwrap()
        }

        fn prop(value: String) -> bool {
            if encode(&value) != reference(&value) {
                return false;
            }

            // Guarantee that QuickCheck exercises a newline without another special
            // byte that could independently select the escaping path.
            let newline_only = format!("{}\n", value.replace(['\\', '"', '\n'], ""));
            encode(&newline_only) == reference(&newline_only)
        }

        QuickCheck::new()
            .tests(1_000)
            .quickcheck(prop as fn(String) -> bool);
    }

    #[test]
    fn oversized_label_value_is_encoded() {
        let value = "v".repeat(3 * FLUSH_THRESHOLD);

        let mut registry = Registry::default();
        let family = Family::<Vec<(String, String)>, Counter>::default();
        registry.register("requests", "Requests", family.clone());
        family
            .get_or_create(&vec![("label".to_string(), value.clone())])
            .inc();

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP requests Requests.\n".to_owned()
            + "# TYPE requests counter\n"
            + "requests_total{label=\""
            + &value
            + "\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);
        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn escaped_label_values_produce_parseable_exposition() {
        let mut registry = Registry::default();
        let family = Family::<Vec<(String, String)>, Counter>::default();
        registry.register("requests", "Requests", family.clone());
        family
            .get_or_create(&vec![(
                "client_version".to_string(),
                "a\"} evil{x=\"1\\line\nnext".to_string(),
            )])
            .inc();

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        assert_eq!(
            concat!(
                "# HELP requests Requests.\n",
                "# TYPE requests counter\n",
                r#"requests_total{client_version="a\"} evil{x=\"1\\line\nnext"} 1"#,
                "\n# EOF\n"
            ),
            encoded
        );
        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_counter() {
        let counter: Counter = Counter::default();
        let mut registry = Registry::default();
        registry.register("my_counter", "My counter", counter);

        let counter_f32 = Counter::<f32, AtomicU32>::default();
        registry.register("f32_counter", "Counter::<f32, AtomicU32>", counter_f32);
        let counter_u32 = Counter::<u32, AtomicU32>::default();
        registry.register("u32_counter", "Counter::<u32, AtomicU32>", counter_u32);
        let mut encoded = String::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_counter_with_unit() {
        let mut registry = Registry::default();
        let counter: Counter = Counter::default();
        registry.register_with_unit("my_counter", "My counter", Unit::Seconds, counter);

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_counter_seconds My counter.\n".to_owned()
            + "# TYPE my_counter_seconds counter\n"
            + "# UNIT my_counter_seconds seconds\n"
            + "my_counter_seconds_total 0\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_counter_with_exemplar() {
        let now = SystemTime::now();

        let mut registry = Registry::default();

        let counter_with_exemplar: CounterWithExemplar<Vec<(String, u64)>> =
            CounterWithExemplar::default();
        registry.register_with_unit(
            "my_counter_with_exemplar",
            "My counter with exemplar",
            Unit::Seconds,
            counter_with_exemplar.clone(),
        );

        counter_with_exemplar.inc_by(1, Some(vec![("user_id".to_string(), 42)]), None);

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_counter_with_exemplar_seconds My counter with exemplar.\n"
            .to_owned()
            + "# TYPE my_counter_with_exemplar_seconds counter\n"
            + "# UNIT my_counter_with_exemplar_seconds seconds\n"
            + "my_counter_with_exemplar_seconds_total 1 # {user_id=\"42\"} 1.0\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);

        counter_with_exemplar.inc_by(1, Some(vec![("user_id".to_string(), 99)]), Some(now));

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_counter_with_exemplar_seconds My counter with exemplar.\n"
            .to_owned()
            + "# TYPE my_counter_with_exemplar_seconds counter\n"
            + "# UNIT my_counter_with_exemplar_seconds seconds\n"
            + "my_counter_with_exemplar_seconds_total 2 # {user_id=\"99\"} 1.0 "
            + dtoa::Buffer::new().format(now.duration_since(UNIX_EPOCH).unwrap().as_secs_f64())
            + "\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_gauge() {
        let mut registry = Registry::default();
        let gauge: Gauge = Gauge::default();
        registry.register("my_gauge", "My gauge", gauge);
        let gauge = Gauge::<u32, AtomicU32>::default();
        registry.register("u32_gauge", "Gauge::<u32, AtomicU32>", gauge);

        let gauge_f32 = Gauge::<f32, AtomicU32>::default();
        registry.register("f32_gauge", "Gauge::<f32, AtomicU32>", gauge_f32);

        let gauge_i32 = Gauge::<i32, AtomicI32>::default();
        registry.register("i32_gauge", "Gauge::<i32, AtomicU32>", gauge_i32);

        let mut encoded = String::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_counter_family() {
        let mut registry = Registry::default();
        let family = Family::<Vec<(String, String)>, Counter>::default();
        registry.register("my_counter_family", "My counter family", family.clone());

        family
            .get_or_create(&vec![
                ("method".to_string(), "GET".to_string()),
                ("status".to_string(), "200".to_string()),
            ])
            .inc();

        let mut encoded = String::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_counter_family_with_prefix_with_label() {
        let mut registry = Registry::default();
        let sub_registry = registry.sub_registry_with_prefix("my_prefix");
        let sub_sub_registry = sub_registry
            .sub_registry_with_label((Cow::Borrowed("my_key"), Cow::Borrowed("my_value")));
        let family = Family::<Vec<(String, String)>, Counter>::default();
        sub_sub_registry.register("my_counter_family", "My counter family", family.clone());

        family
            .get_or_create(&vec![
                ("method".to_string(), "GET".to_string()),
                ("status".to_string(), "200".to_string()),
            ])
            .inc();

        let mut encoded = String::new();

        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_prefix_my_counter_family My counter family.\n"
            .to_owned()
            + "# TYPE my_prefix_my_counter_family counter\n"
            + "my_prefix_my_counter_family_total{my_key=\"my_value\",method=\"GET\",status=\"200\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_info() {
        let mut registry = Registry::default();
        let info = Info::new(vec![("os".to_string(), "GNU/linux".to_string())]);
        registry.register("my_info_metric", "My info metric", info);

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_info_metric My info metric.\n".to_owned()
            + "# TYPE my_info_metric info\n"
            + "my_info_metric_info{os=\"GNU/linux\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_info_with_const_labels_and_empty_label_set() {
        let mut registry =
            Registry::with_labels(vec![(Cow::Borrowed("env"), Cow::Borrowed("prod"))].into_iter());
        let info = Info::new(Vec::<(String, String)>::new());
        registry.register("my_info_metric", "My info metric", info);

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_info_metric My info metric.\n".to_owned()
            + "# TYPE my_info_metric info\n"
            + "my_info_metric_info{env=\"prod\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_info_with_const_and_family_labels() {
        let mut registry = Registry::default();
        let sub_registry =
            registry.sub_registry_with_label((Cow::Borrowed("region"), Cow::Borrowed("eu")));
        let infos =
            Family::<Vec<(String, String)>, Info<Vec<(String, String)>>, _>::new_with_constructor(
                || Info::new(vec![("version".to_string(), "1.2".to_string())]),
            );
        sub_registry.register("my_info_metric", "My info metric", infos.clone());
        let _ = infos.get_or_create(&vec![("method".to_string(), "GET".to_string())]);

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_info_metric My info metric.\n".to_owned()
            + "# TYPE my_info_metric info\n"
            + "my_info_metric_info{region=\"eu\",method=\"GET\",version=\"1.2\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_histogram_with_const_and_family_labels() {
        let mut registry = Registry::default();
        let sub_registry =
            registry.sub_registry_with_label((Cow::Borrowed("region"), Cow::Borrowed("eu")));
        let histograms = Family::<Vec<(String, String)>, Histogram>::new_with_constructor(|| {
            Histogram::new([1.0])
        });
        sub_registry.register("my_histogram", "My histogram", histograms.clone());
        histograms
            .get_or_create(&vec![("path".to_string(), "a\"b\\c\nd".to_string())])
            .observe(2.0);

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let labels = r#"region="eu",path="a\"b\\c\nd""#;
        let expected = "# HELP my_histogram My histogram.\n".to_owned()
            + "# TYPE my_histogram histogram\n"
            + &format!("my_histogram_sum{{{labels}}} 2.0\n")
            + &format!("my_histogram_count{{{labels}}} 1\n")
            + &format!("my_histogram_bucket{{{labels},le=\"1.0\"}} 0\n")
            + &format!("my_histogram_bucket{{{labels},le=\"+Inf\"}} 1\n")
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_consecutive_histograms_with_different_labels() {
        let mut registry = Registry::default();
        let first = Histogram::new([1.0]);
        registry
            .sub_registry_with_label((Cow::Borrowed("a"), Cow::Borrowed("1")))
            .register("first", "First", first.clone());
        let second = Histogram::new([1.0]);
        registry
            .sub_registry_with_label((Cow::Borrowed("b"), Cow::Borrowed("2")))
            .register("second", "Second", second.clone());
        let third = Histogram::new([1.0]);
        registry.register("third", "Third", third.clone());
        first.observe(0.5);
        second.observe(0.5);
        third.observe(0.5);

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP third Third.\n".to_owned()
            + "# TYPE third histogram\n"
            + "third_sum 0.5\n"
            + "third_count 1\n"
            + "third_bucket{le=\"1.0\"} 1\n"
            + "third_bucket{le=\"+Inf\"} 1\n"
            + "# HELP first First.\n"
            + "# TYPE first histogram\n"
            + "first_sum{a=\"1\"} 0.5\n"
            + "first_count{a=\"1\"} 1\n"
            + "first_bucket{a=\"1\",le=\"1.0\"} 1\n"
            + "first_bucket{a=\"1\",le=\"+Inf\"} 1\n"
            + "# HELP second Second.\n"
            + "# TYPE second histogram\n"
            + "second_sum{b=\"2\"} 0.5\n"
            + "second_count{b=\"2\"} 1\n"
            + "second_bucket{b=\"2\",le=\"1.0\"} 1\n"
            + "second_bucket{b=\"2\",le=\"+Inf\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_labels_skipping_label_key() {
        /// Label which only encodes a key when present.
        #[derive(Clone, Debug, Hash, PartialEq, Eq)]
        struct OptionalLabel(Option<(&'static str, &'static str)>);

        impl crate::encoding::EncodeLabel for OptionalLabel {
            fn encode(&self, encoder: crate::encoding::LabelEncoder) -> Result<(), Error> {
                match self.0 {
                    Some(label) => label.encode(encoder),
                    None => Ok(()),
                }
            }
        }

        fn encode_info(
            const_labels: Vec<(Cow<'static, str>, Cow<'static, str>)>,
            labels: Vec<OptionalLabel>,
        ) -> String {
            let mut registry = Registry::with_labels(const_labels.into_iter());
            registry.register("my_info_metric", "My info metric", Info::new(labels));
            let mut encoded = String::new();
            encode(&mut encoded, &registry).unwrap();
            encoded
        }

        fn expected(series: &str) -> String {
            "# HELP my_info_metric My info metric.\n".to_owned()
                + "# TYPE my_info_metric info\n"
                + series
                + "\n# EOF\n"
        }

        let cases = [
            (vec![], vec![OptionalLabel(None)], "my_info_metric_info 1"),
            (
                vec![],
                vec![
                    OptionalLabel(None),
                    OptionalLabel(Some(("a", "1"))),
                    OptionalLabel(None),
                    OptionalLabel(Some(("b", "2"))),
                ],
                "my_info_metric_info{a=\"1\",b=\"2\"} 1",
            ),
            (
                vec![(Cow::Borrowed("env"), Cow::Borrowed("prod"))],
                vec![OptionalLabel(None)],
                "my_info_metric_info{env=\"prod\"} 1",
            ),
        ];

        for (const_labels, labels, series) in cases {
            let encoded = encode_info(const_labels, labels);
            assert_eq!(expected(series), encoded);
            parse_with_python_client(Parser::OpenMetricsParser, &encoded);
        }
    }

    #[test]
    fn encode_histogram() {
        let mut registry = Registry::default();
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);

        let mut encoded = String::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_classic_and_native_histogram_as_classic_text() {
        let mut registry = Registry::default();
        let histogram = Histogram::new_classic_and_native(
            [1.0, 2.0],
            NativeHistogramConfig::with_bucket_factor(1.1),
        );
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_histogram My histogram.\n".to_owned()
            + "# TYPE my_histogram histogram\n"
            + "my_histogram_sum 1.0\n"
            + "my_histogram_count 1\n"
            + "my_histogram_bucket{le=\"1.0\"} 1\n"
            + "my_histogram_bucket{le=\"2.0\"} 1\n"
            + "my_histogram_bucket{le=\"+Inf\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);
    }

    #[test]
    fn encode_native_only_histogram_errors() {
        let mut registry = Registry::default();
        let histogram = Histogram::new_native(NativeHistogramConfig::with_schema(0));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);

        let mut encoded = String::new();
        assert!(encode(&mut encoded, &registry).is_err());
    }

    #[test]
    fn encode_histogram_family() {
        let mut registry = Registry::default();
        let family =
            Family::new_with_constructor(|| Histogram::new(exponential_buckets(1.0, 2.0, 10)));
        registry.register("my_histogram", "My histogram", family.clone());
        family
            .get_or_create(&vec![
                ("method".to_string(), "GET".to_string()),
                ("status".to_string(), "200".to_string()),
            ])
            .observe(1.0);

        let mut encoded = String::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_histogram_family_with_empty_struct_family_labels() {
        let mut registry = Registry::default();
        let family =
            Family::new_with_constructor(|| Histogram::new(exponential_buckets(1.0, 2.0, 10)));
        registry.register("my_histogram", "My histogram", family.clone());

        #[derive(Eq, PartialEq, Hash, Debug, Clone)]
        struct EmptyLabels {}

        impl EncodeLabelSet for EmptyLabels {
            fn encode(&self, _encoder: &mut crate::encoding::LabelSetEncoder) -> Result<(), Error> {
                Ok(())
            }
        }

        family.get_or_create(&EmptyLabels {}).observe(1.0);

        let mut encoded = String::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_histogram_with_exemplars() {
        let now = SystemTime::now();

        let mut registry = Registry::default();
        let histogram = HistogramWithExemplars::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());

        histogram.observe(1.0, Some([("user_id".to_string(), 42u64)]), Some(now));
        histogram.observe(2.0, Some([("user_id".to_string(), 99u64)]), None);

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_histogram My histogram.\n".to_owned()
            + "# TYPE my_histogram histogram\n"
            + "my_histogram_sum 3.0\n"
            + "my_histogram_count 2\n"
            + "my_histogram_bucket{le=\"1.0\"} 1 # {user_id=\"42\"} 1.0 "
            + dtoa::Buffer::new().format(now.duration_since(UNIX_EPOCH).unwrap().as_secs_f64())
            + "\n"
            + "my_histogram_bucket{le=\"2.0\"} 2 # {user_id=\"99\"} 2.0\n"
            + "my_histogram_bucket{le=\"4.0\"} 2\n"
            + "my_histogram_bucket{le=\"8.0\"} 2\n"
            + "my_histogram_bucket{le=\"16.0\"} 2\n"
            + "my_histogram_bucket{le=\"32.0\"} 2\n"
            + "my_histogram_bucket{le=\"64.0\"} 2\n"
            + "my_histogram_bucket{le=\"128.0\"} 2\n"
            + "my_histogram_bucket{le=\"256.0\"} 2\n"
            + "my_histogram_bucket{le=\"512.0\"} 2\n"
            + "my_histogram_bucket{le=\"+Inf\"} 2\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn sub_registry_with_prefix_and_label() {
        let top_level_metric_name = "my_top_level_metric";
        let mut registry = Registry::default();
        let counter: Counter = Counter::default();
        registry.register(top_level_metric_name, "some help", counter.clone());

        let prefix_1 = "prefix_1";
        let prefix_1_metric_name = "my_prefix_1_metric";
        let sub_registry = registry.sub_registry_with_prefix(prefix_1);
        sub_registry.register(prefix_1_metric_name, "some help", counter.clone());

        let prefix_1_1 = "prefix_1_1";
        let prefix_1_1_metric_name = "my_prefix_1_1_metric";
        let sub_sub_registry = sub_registry.sub_registry_with_prefix(prefix_1_1);
        sub_sub_registry.register(prefix_1_1_metric_name, "some help", counter.clone());

        let label_1_2 = (Cow::Borrowed("registry"), Cow::Borrowed("1_2"));
        let prefix_1_2_metric_name = "my_prefix_1_2_metric";
        let sub_sub_registry = sub_registry.sub_registry_with_label(label_1_2.clone());
        sub_sub_registry.register(prefix_1_2_metric_name, "some help", counter.clone());

        let labels_1_3 = vec![
            (Cow::Borrowed("label_1_3_1"), Cow::Borrowed("value_1_3_1")),
            (Cow::Borrowed("label_1_3_2"), Cow::Borrowed("value_1_3_2")),
        ];
        let prefix_1_3_metric_name = "my_prefix_1_3_metric";
        let sub_sub_registry =
            sub_registry.sub_registry_with_labels(labels_1_3.clone().into_iter());
        sub_sub_registry.register(prefix_1_3_metric_name, "some help", counter.clone());

        let prefix_1_3_1 = "prefix_1_3_1";
        let prefix_1_3_1_metric_name = "my_prefix_1_3_1_metric";
        let sub_sub_sub_registry = sub_sub_registry.sub_registry_with_prefix(prefix_1_3_1);
        sub_sub_sub_registry.register(prefix_1_3_1_metric_name, "some help", counter.clone());

        let prefix_2 = "prefix_2";
        let _ = registry.sub_registry_with_prefix(prefix_2);

        let prefix_3 = "prefix_3";
        let prefix_3_metric_name = "my_prefix_3_metric";
        let sub_registry = registry.sub_registry_with_prefix(prefix_3);
        sub_registry.register(prefix_3_metric_name, "some help", counter);

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_top_level_metric some help.\n".to_owned()
            + "# TYPE my_top_level_metric counter\n"
            + "my_top_level_metric_total 0\n"
            + "# HELP prefix_1_my_prefix_1_metric some help.\n"
            + "# TYPE prefix_1_my_prefix_1_metric counter\n"
            + "prefix_1_my_prefix_1_metric_total 0\n"
            + "# HELP prefix_1_prefix_1_1_my_prefix_1_1_metric some help.\n"
            + "# TYPE prefix_1_prefix_1_1_my_prefix_1_1_metric counter\n"
            + "prefix_1_prefix_1_1_my_prefix_1_1_metric_total 0\n"
            + "# HELP prefix_1_my_prefix_1_2_metric some help.\n"
            + "# TYPE prefix_1_my_prefix_1_2_metric counter\n"
            + "prefix_1_my_prefix_1_2_metric_total{registry=\"1_2\"} 0\n"
            + "# HELP prefix_1_my_prefix_1_3_metric some help.\n"
            + "# TYPE prefix_1_my_prefix_1_3_metric counter\n"
            + "prefix_1_my_prefix_1_3_metric_total{label_1_3_1=\"value_1_3_1\",label_1_3_2=\"value_1_3_2\"} 0\n"
            + "# HELP prefix_1_prefix_1_3_1_my_prefix_1_3_1_metric some help.\n"
            + "# TYPE prefix_1_prefix_1_3_1_my_prefix_1_3_1_metric counter\n"
            + "prefix_1_prefix_1_3_1_my_prefix_1_3_1_metric_total{label_1_3_1=\"value_1_3_1\",label_1_3_2=\"value_1_3_2\"} 0\n"
            + "# HELP prefix_3_my_prefix_3_metric some help.\n"
            + "# TYPE prefix_3_my_prefix_3_metric counter\n"
            + "prefix_3_my_prefix_3_metric_total 0\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn sub_registry_collector() {
        use crate::encoding::EncodeMetric;

        #[derive(Debug)]
        struct Collector {
            name: String,
        }

        impl Collector {
            fn new(name: impl Into<String>) -> Self {
                Self { name: name.into() }
            }
        }

        impl crate::collector::Collector for Collector {
            fn encode(
                &self,
                mut encoder: crate::encoding::DescriptorEncoder,
            ) -> Result<(), std::fmt::Error> {
                let counter = crate::metrics::counter::ConstCounter::new(42u64);
                let metric_encoder = encoder.encode_descriptor(
                    &self.name,
                    "some help",
                    None,
                    counter.metric_type(),
                )?;
                counter.encode(metric_encoder)?;
                Ok(())
            }
        }

        let mut registry = Registry::default();
        registry.register_collector(Box::new(Collector::new("top_level")));

        let sub_registry = registry.sub_registry_with_prefix("prefix_1");
        sub_registry.register_collector(Box::new(Collector::new("sub_level")));

        let sub_sub_registry = sub_registry.sub_registry_with_prefix("prefix_1_2");
        sub_sub_registry.register_collector(Box::new(Collector::new("sub_sub_level")));

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP top_level some help\n".to_owned()
            + "# TYPE top_level counter\n"
            + "top_level_total 42\n"
            + "# HELP prefix_1_sub_level some help\n"
            + "# TYPE prefix_1_sub_level counter\n"
            + "prefix_1_sub_level_total 42\n"
            + "# HELP prefix_1_prefix_1_2_sub_sub_level some help\n"
            + "# TYPE prefix_1_prefix_1_2_sub_sub_level counter\n"
            + "prefix_1_prefix_1_2_sub_sub_level_total 42\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn label_sets_can_be_composed() {
        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
        struct Color(&'static str);
        impl EncodeLabelSet for Color {
            fn encode(
                &self,
                encoder: &mut crate::encoding::LabelSetEncoder,
            ) -> Result<(), std::fmt::Error> {
                use crate::encoding::EncodeLabel;
                let Self(color) = *self;
                let labels = ("color", color);
                let encoder = encoder.encode_label();
                labels.encode(encoder)
            }
        }

        #[derive(Clone, Debug, Eq, Hash, PartialEq)]
        struct Size(&'static str);
        impl EncodeLabelSet for Size {
            fn encode(
                &self,
                encoder: &mut crate::encoding::LabelSetEncoder,
            ) -> Result<(), std::fmt::Error> {
                use crate::encoding::EncodeLabel;
                let Self(size) = *self;
                let labels = ("size", size);
                let encoder = encoder.encode_label();
                labels.encode(encoder)
            }
        }

        type Labels = (Color, Size);

        let mut registry = Registry::default();
        let family = Family::<Labels, Counter>::default();
        registry.register("items", "Example metric", family.clone());

        let labels = (Color("red"), Size("large"));
        let counter = family.get_or_create(&labels);
        counter.inc();

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP items Example metric.\n\
                        # TYPE items counter\n\
                        items_total{color=\"red\",size=\"large\"} 1\n\
                        # EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(Parser::OpenMetricsParser, &encoded);
    }

    #[test]
    fn encode_registry_eof() {
        let mut orders_registry = Registry::default();

        let total_orders: Counter<u64> = Default::default();
        orders_registry.register("orders", "Total orders received", total_orders.clone());
        total_orders.inc();

        let processing_times = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        orders_registry.register_with_unit(
            "processing_times",
            "Order times",
            Unit::Seconds,
            processing_times.clone(),
        );
        processing_times.observe(2.4);

        let mut user_auth_registry = Registry::default();

        let successful_logins: Counter<u64> = Default::default();
        user_auth_registry.register(
            "successful_logins",
            "Total successful logins",
            successful_logins.clone(),
        );
        successful_logins.inc();

        let failed_logins: Counter<u64> = Default::default();
        user_auth_registry.register(
            "failed_logins",
            "Total failed logins",
            failed_logins.clone(),
        );

        let mut response = String::new();

        encode_registry(&mut response, &orders_registry).unwrap();
        assert_eq!(&response[response.len() - 20..], "bucket{le=\"+Inf\"} 1\n");

        encode_registry(&mut response, &user_auth_registry).unwrap();
        assert_eq!(&response[response.len() - 20..], "iled_logins_total 0\n");

        encode_eof(&mut response).unwrap();
        assert_eq!(&response[response.len() - 20..], "ogins_total 0\n# EOF\n");
    }

    #[test]
    fn encode_flushes_large_output_in_chunks() {
        /// Writer recording each `write_str` call separately.
        #[derive(Default)]
        struct ChunkRecorder(Vec<String>);

        impl Write for ChunkRecorder {
            fn write_str(&mut self, s: &str) -> std::fmt::Result {
                self.0.push(s.to_owned());
                Ok(())
            }
        }

        let mut registry = Registry::default();
        for i in 0..200 {
            let family = Family::<Vec<(String, String)>, Counter>::default();
            registry.register(format!("my_counter_{i}"), "My counter", family.clone());
            for j in 0..10 {
                family
                    .get_or_create(&vec![("label".to_string(), j.to_string())])
                    .inc();
            }
        }

        let mut expected = String::new();
        encode(&mut expected, &registry).unwrap();
        assert!(expected.len() > 2 * FLUSH_THRESHOLD);

        let mut recorder = ChunkRecorder::default();
        encode(&mut recorder, &registry).unwrap();

        // The output is flushed in multiple chunks of at least
        // `FLUSH_THRESHOLD` bytes (except for the trailing remainder and the
        // EOF marker), and the concatenation matches.
        let (chunks, tail) = recorder.0.split_at(recorder.0.len() - 2);
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| chunk.len() >= FLUSH_THRESHOLD));
        assert!(tail[0].len() < FLUSH_THRESHOLD);
        assert_eq!(tail[1], "# EOF\n");
        assert_eq!(recorder.0.concat(), expected);

        parse_with_python_client(Parser::PrometheusClientParser, &expected);
    }

    #[test]
    fn encode_omit_empty() {
        let mut registry = Registry::default();
        let counter1: Family<Vec<(&'static str, &'static str)>, Counter> = Default::default();
        let counter2: Family<Vec<(&'static str, &'static str)>, Counter> = Default::default();
        let counter3: Family<Vec<(&'static str, &'static str)>, Counter> = Default::default();

        registry.register("counter1", "First counter", counter1.clone());
        registry.register("counter2", "Second counter", counter2.clone());
        registry.register("counter3", "Third counter", counter3.clone());

        counter1.get_or_create(&vec![("label", "value")]).inc();

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP counter1 First counter.\n".to_owned()
            + "# TYPE counter1 counter\n"
            + "counter1_total{label=\"value\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        counter2.get_or_create(&vec![("label", "value")]).inc();

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP counter1 First counter.\n".to_owned()
            + "# TYPE counter1 counter\n"
            + "counter1_total{label=\"value\"} 1\n"
            + "# HELP counter2 Second counter.\n"
            + "# TYPE counter2 counter\n"
            + "counter2_total{label=\"value\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);
    }
}
