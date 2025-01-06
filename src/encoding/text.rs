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

use crate::encoding::{EncodeExemplarValue, EncodeLabelSet, NoLabelSet};
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
    registry.encode(&mut DescriptorEncoder::new(writer).into())
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
    writer: &'a mut dyn Write,
    prefix: Option<&'a Prefix>,
    labels: &'a [(Cow<'static, str>, Cow<'static, str>)],
}

impl std::fmt::Debug for DescriptorEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DescriptorEncoder").finish()
    }
}

impl DescriptorEncoder<'_> {
    pub(crate) fn new(writer: &mut dyn Write) -> DescriptorEncoder {
        DescriptorEncoder {
            writer,
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
            writer: self.writer,
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
            writer: self.writer,
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
    writer: &'a mut dyn Write,
    prefix: Option<&'a Prefix>,
    name: &'a str,
    unit: Option<&'a Unit>,
    const_labels: &'a [(Cow<'static, str>, Cow<'static, str>)],
    family_labels: Option<&'a dyn super::EncodeLabelSet>,
}

impl std::fmt::Debug for MetricEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut labels = String::new();
        if let Some(l) = self.family_labels {
            l.encode(LabelSetEncoder::new(&mut labels).into())?;
        }

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
                writer: self.writer,
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
                writer: self.writer,
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
            writer: self.writer,
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
        self.write_prefix_name_unit()?;
        self.write_suffix("sum")?;
        self.encode_labels::<NoLabelSet>(None)?;
        self.writer.write_str(" ")?;
        self.writer.write_str(dtoa::Buffer::new().format(sum))?;
        self.newline()?;

        self.write_prefix_name_unit()?;
        self.write_suffix("count")?;
        self.encode_labels::<NoLabelSet>(None)?;
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(count))?;
        self.newline()?;

        let mut cummulative = 0;
        for (i, (upper_bound, count)) in buckets.iter().enumerate() {
            cummulative += count;

            self.write_prefix_name_unit()?;
            self.write_suffix("bucket")?;

            if *upper_bound == f64::MAX {
                self.encode_labels(Some(&[("le", "+Inf")]))?;
            } else {
                self.encode_labels(Some(&[("le", *upper_bound)]))?;
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

    /// Encode an exemplar for the given metric.
    fn encode_exemplar<S: EncodeLabelSet, V: EncodeExemplarValue>(
        &mut self,
        exemplar: &Exemplar<S, V>,
    ) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" # {")?;
        exemplar
            .label_set
            .encode(LabelSetEncoder::new(self.writer).into())?;
        self.writer.write_str("} ")?;
        exemplar.value.encode(
            ExemplarValueEncoder {
                writer: self.writer,
            }
            .into(),
        )?;
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

    // TODO: Consider caching the encoded labels for Histograms as they stay the
    // same but are currently encoded multiple times.
    fn encode_labels<S: EncodeLabelSet>(
        &mut self,
        additional_labels: Option<&S>,
    ) -> Result<(), std::fmt::Error> {
        if self.const_labels.is_empty()
            && additional_labels.is_none()
            && self.family_labels.is_none()
        {
            return Ok(());
        }

        self.writer.write_str("{")?;

        self.const_labels
            .encode(LabelSetEncoder::new(self.writer).into())?;

        if let Some(additional_labels) = additional_labels {
            if !self.const_labels.is_empty() {
                self.writer.write_str(",")?;
            }

            additional_labels.encode(LabelSetEncoder::new(self.writer).into())?;
        }

        /// Writer impl which prepends a comma on the first call to write output to the wrapped writer
        struct CommaPrependingWriter<'a> {
            writer: &'a mut dyn Write,
            should_prepend: bool,
        }

        impl Write for CommaPrependingWriter<'_> {
            fn write_str(&mut self, s: &str) -> std::fmt::Result {
                if self.should_prepend {
                    self.writer.write_char(',')?;
                    self.should_prepend = false;
                }
                self.writer.write_str(s)
            }
        }

        if let Some(labels) = self.family_labels {
            // if const labels or additional labels have been written, a comma must be prepended before writing the family labels.
            // However, it could be the case that the family labels are `Some` and yet empty, so the comma should _only_
            // be prepended if one of the `Write` methods are actually called when attempting to write the family labels.
            // Therefore, wrap the writer on `Self` with a CommaPrependingWriter if other labels have been written and
            // there may be a need to prepend an extra comma before writing additional labels.
            if !self.const_labels.is_empty() || additional_labels.is_some() {
                let mut writer = CommaPrependingWriter {
                    writer: self.writer,
                    should_prepend: true,
                };
                labels.encode(LabelSetEncoder::new(&mut writer).into())?;
            } else {
                labels.encode(LabelSetEncoder::new(self.writer).into())?;
            };
        }

        self.writer.write_str("}")?;

        Ok(())
    }
}

pub(crate) struct CounterValueEncoder<'a> {
    writer: &'a mut dyn Write,
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
    writer: &'a mut dyn Write,
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
    writer: &'a mut dyn Write,
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

pub(crate) struct LabelSetEncoder<'a> {
    writer: &'a mut dyn Write,
    first: bool,
}

impl std::fmt::Debug for LabelSetEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelSetEncoder")
            .field("first", &self.first)
            .finish()
    }
}

impl<'a> LabelSetEncoder<'a> {
    fn new(writer: &'a mut dyn Write) -> Self {
        Self {
            writer,
            first: true,
        }
    }

    pub fn encode_label(&mut self) -> LabelEncoder {
        let first = self.first;
        self.first = false;
        LabelEncoder {
            writer: self.writer,
            first,
        }
    }
}

pub(crate) struct LabelEncoder<'a> {
    writer: &'a mut dyn Write,
    first: bool,
}

impl std::fmt::Debug for LabelEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelEncoder")
            .field("first", &self.first)
            .finish()
    }
}

impl LabelEncoder<'_> {
    pub fn encode_label_key(&mut self) -> Result<LabelKeyEncoder, std::fmt::Error> {
        if !self.first {
            self.writer.write_str(",")?;
        }
        Ok(LabelKeyEncoder {
            writer: self.writer,
        })
    }
}

pub(crate) struct LabelKeyEncoder<'a> {
    writer: &'a mut dyn Write,
}

impl std::fmt::Debug for LabelKeyEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelKeyEncoder").finish()
    }
}

impl<'a> LabelKeyEncoder<'a> {
    pub fn encode_label_value(self) -> Result<LabelValueEncoder<'a>, std::fmt::Error> {
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
    writer: &'a mut dyn Write,
}

impl std::fmt::Debug for LabelValueEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelValueEncoder").finish()
    }
}

impl LabelValueEncoder<'_> {
    pub fn finish(self) -> Result<(), std::fmt::Error> {
        self.writer.write_str("\"")
    }
}

impl std::fmt::Write for LabelValueEncoder<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.writer.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::exemplar::HistogramWithExemplars;
    use crate::metrics::family::Family;
    use crate::metrics::gauge::Gauge;
    use crate::metrics::histogram::{exponential_buckets, Histogram};
    use crate::metrics::info::Info;
    use crate::metrics::{counter::Counter, exemplar::CounterWithExemplar};
    use pyo3::{prelude::*, types::PyModule};
    use std::borrow::Cow;
    use std::fmt::Error;
    use std::sync::atomic::{AtomicI32, AtomicU32};

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

        parse_with_python_client(encoded);
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

        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_counter_with_exemplar() {
        let mut registry = Registry::default();

        let counter_with_exemplar: CounterWithExemplar<Vec<(String, u64)>> =
            CounterWithExemplar::default();
        registry.register_with_unit(
            "my_counter_with_exemplar",
            "My counter with exemplar",
            Unit::Seconds,
            counter_with_exemplar.clone(),
        );

        counter_with_exemplar.inc_by(1, Some(vec![("user_id".to_string(), 42)]));

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_counter_with_exemplar_seconds My counter with exemplar.\n"
            .to_owned()
            + "# TYPE my_counter_with_exemplar_seconds counter\n"
            + "# UNIT my_counter_with_exemplar_seconds seconds\n"
            + "my_counter_with_exemplar_seconds_total 1 # {user_id=\"42\"} 1.0\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(encoded);
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

        parse_with_python_client(encoded);
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

        parse_with_python_client(encoded);
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

        parse_with_python_client(encoded);
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

        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_histogram() {
        let mut registry = Registry::default();
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);

        let mut encoded = String::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(encoded);
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

        parse_with_python_client(encoded);
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
            fn encode(&self, _encoder: crate::encoding::LabelSetEncoder) -> Result<(), Error> {
                Ok(())
            }
        }

        family.get_or_create(&EmptyLabels {}).observe(1.0);

        let mut encoded = String::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_histogram_with_exemplars() {
        let mut registry = Registry::default();
        let histogram = HistogramWithExemplars::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0, Some([("user_id".to_string(), 42u64)]));

        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_histogram My histogram.\n".to_owned()
            + "# TYPE my_histogram histogram\n"
            + "my_histogram_sum 1.0\n"
            + "my_histogram_count 1\n"
            + "my_histogram_bucket{le=\"1.0\"} 1 # {user_id=\"42\"} 1.0\n"
            + "my_histogram_bucket{le=\"2.0\"} 1\n"
            + "my_histogram_bucket{le=\"4.0\"} 1\n"
            + "my_histogram_bucket{le=\"8.0\"} 1\n"
            + "my_histogram_bucket{le=\"16.0\"} 1\n"
            + "my_histogram_bucket{le=\"32.0\"} 1\n"
            + "my_histogram_bucket{le=\"64.0\"} 1\n"
            + "my_histogram_bucket{le=\"128.0\"} 1\n"
            + "my_histogram_bucket{le=\"256.0\"} 1\n"
            + "my_histogram_bucket{le=\"512.0\"} 1\n"
            + "my_histogram_bucket{le=\"+Inf\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, encoded);

        parse_with_python_client(encoded);
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

        parse_with_python_client(encoded);
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

        parse_with_python_client(encoded);
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

    fn parse_with_python_client(input: String) {
        pyo3::prepare_freethreaded_python();

        println!("{:?}", input);
        Python::with_gil(|py| {
            let parser = PyModule::from_code_bound(
                py,
                r#"
from prometheus_client.openmetrics.parser import text_string_to_metric_families

def parse(input):
    families = text_string_to_metric_families(input)
    list(families)
"#,
                "parser.py",
                "parser",
            )
            .map_err(|e| e.to_string())
            .unwrap();

            parser
                .getattr("parse")
                .expect("`parse` to exist.")
                .call1((input.clone(),))
                .map_err(|e| e.to_string())
                .unwrap();
        })
    }
}
