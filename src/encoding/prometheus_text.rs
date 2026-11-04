//! Prometheus text format (version 0.0.4) implementation.
//!
//! ```
//! # use prometheus_client::encoding::prometheus_text::encode;
//! # use prometheus_client::metrics::counter::Counter;
//! # use prometheus_client::registry::Registry;
//! #
//! # let mut registry = Registry::default();
//! # let counter: Counter = Counter::default();
//! # registry.register(
//! #   "my_counter",
//! #   "This is my counter",
//! #   counter.clone(),
//! # );
//! # counter.inc();
//! let mut buffer = String::new();
//! encode(&mut buffer, &registry).unwrap();
//!
//! let expected = "# HELP my_counter_total This is my counter.\n".to_owned() +
//!                "# TYPE my_counter_total counter\n" +
//!                "my_counter_total 1\n";
//! assert_eq!(expected, buffer);
//! ```

use crate::encoding::{text, EncodeExemplarValue, EncodeLabelSet, NativeHistogram, NoLabelSet};
use crate::metrics::exemplar::Exemplar;
use crate::metrics::MetricType;
use crate::registry::{Prefix, Registry, Unit};

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write;

/// The `Content-Type` of the Prometheus text format (version 0.0.4).
///
/// Equivalent to `expfmt.FmtText` in Go's
/// [`prometheus/common`](https://github.com/prometheus/common/blob/main/expfmt/expfmt.go).
pub const CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// Encode the metrics registered with the provided [`Registry`] into the
/// provided [`Write`]r using the Prometheus text format (version 0.0.4).
///
/// # Examples
///
/// ```no_run
/// # use prometheus_client::encoding::prometheus_text::encode;
/// # use prometheus_client::metrics::counter::Counter;
/// # use prometheus_client::registry::Registry;
/// #
/// let mut registry = Registry::default();
/// let counter: Counter = Counter::default();
/// registry.register("my_counter", "This is my counter", counter.clone());
///
/// let mut buffer = String::new();
/// encode(&mut buffer, &registry)?;
/// # Ok::<(), std::fmt::Error>(())
/// ```
pub fn encode<W>(writer: &mut W, registry: &Registry) -> Result<(), std::fmt::Error>
where
    W: Write,
{
    registry.encode(&mut DescriptorEncoder::new(writer).into())
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
    pub(crate) fn new(writer: &mut dyn Write) -> DescriptorEncoder<'_> {
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
        self.metric_encoder(name, unit, metric_type)
            .write_name("")?;
        self.writer.write_str(" ")?;
        let needs_escaping = help
            .bytes()
            .fold(false, |found, byte| found | matches!(byte, b'\\' | b'\n'));
        if needs_escaping {
            self.write_escaped(help)?;
        } else {
            self.writer.write_str(help)?;
        }
        self.writer.write_str("\n")?;

        let mut encoder = self.metric_encoder(name, unit, metric_type);
        encoder.writer.write_str("# TYPE ")?;
        encoder.write_name("")?;
        encoder.writer.write_str(match metric_type {
            MetricType::Counter => " counter\n",
            MetricType::Gauge | MetricType::Info => " gauge\n",
            MetricType::Histogram => " histogram\n",
            MetricType::Unknown => " untyped\n",
        })?;

        Ok(encoder)
    }

    fn metric_encoder<'s>(
        &'s mut self,
        name: &'s str,
        unit: Option<&'s Unit>,
        metric_type: MetricType,
    ) -> MetricEncoder<'s> {
        MetricEncoder {
            writer: self.writer,
            prefix: self.prefix,
            name,
            unit,
            metric_type,
            const_labels: self.labels,
            family_labels: None,
        }
    }

    #[cold]
    fn write_escaped(&mut self, help: &str) -> Result<(), std::fmt::Error> {
        let mut last = 0;
        for (index, byte) in help.bytes().enumerate() {
            let escaped = match byte {
                b'\\' => "\\\\",
                b'\n' => "\\n",
                _ => continue,
            };
            self.writer.write_str(&help[last..index])?;
            self.writer.write_str(escaped)?;
            last = index + 1;
        }
        self.writer.write_str(&help[last..])
    }
}

pub(crate) struct MetricEncoder<'a> {
    writer: &'a mut dyn Write,
    prefix: Option<&'a Prefix>,
    name: &'a str,
    unit: Option<&'a Unit>,
    metric_type: MetricType,
    const_labels: &'a [(Cow<'static, str>, Cow<'static, str>)],
    family_labels: Option<&'a dyn EncodeLabelSet>,
}

impl std::fmt::Debug for MetricEncoder<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut labels = String::new();
        let mut encoder = text::LabelSetEncoder::new(&mut labels).into();
        if let Some(l) = self.family_labels {
            l.encode(&mut encoder)?;
        }

        f.debug_struct("Encoder")
            .field("name", &self.name)
            .field("prefix", &self.prefix)
            .field("unit", &self.unit)
            .field("metric_type", &self.metric_type.as_str())
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
        _exemplar: Option<&Exemplar<S, ExemplarValue>>,
    ) -> Result<(), std::fmt::Error> {
        self.write_name("")?;
        self.write_labels::<NoLabelSet>(None)?;
        v.encode(
            &mut CounterValueEncoder {
                writer: self.writer,
            }
            .into(),
        )?;
        self.writer.write_str("\n")
    }

    pub fn encode_gauge<GaugeValue: super::EncodeGaugeValue>(
        &mut self,
        v: &GaugeValue,
    ) -> Result<(), std::fmt::Error> {
        self.write_name("")?;
        self.write_labels::<NoLabelSet>(None)?;
        v.encode(
            &mut GaugeValueEncoder {
                writer: self.writer,
            }
            .into(),
        )?;
        self.writer.write_str("\n")
    }

    pub fn encode_info<S: EncodeLabelSet>(&mut self, label_set: &S) -> Result<(), std::fmt::Error> {
        self.write_name("")?;
        self.write_labels(Some(label_set))?;
        self.writer.write_str(" 1\n")
    }

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
            metric_type: self.metric_type,
            const_labels: self.const_labels,
            family_labels: Some(label_set),
        })
    }

    pub fn encode_histogram<S: EncodeLabelSet>(
        &mut self,
        sum: f64,
        count: u64,
        buckets: &[(f64, u64)],
        _exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
    ) -> Result<(), std::fmt::Error> {
        let mut cumulative = 0;
        let mut inf_seen = false;
        for (upper_bound, count) in buckets {
            cumulative += count;
            self.write_name("_bucket")?;
            if *upper_bound == f64::INFINITY {
                inf_seen = true;
                self.write_labels(Some(&[("le", "+Inf")]))?;
            } else {
                self.write_labels(Some(&[("le", *upper_bound)]))?;
            }
            self.write_u64(cumulative)?;
        }

        if !inf_seen {
            self.write_name("_bucket")?;
            self.write_labels(Some(&[("le", "+Inf")]))?;
            self.write_u64(count)?;
        }

        self.write_name("_sum")?;
        self.write_labels::<NoLabelSet>(None)?;
        self.writer.write_str(" ")?;
        self.writer
            .write_str(format_float(&mut dtoa::Buffer::new(), sum))?;
        self.writer.write_str("\n")?;

        self.write_name("_count")?;
        self.write_labels::<NoLabelSet>(None)?;
        self.write_u64(count)
    }

    pub fn encode_histogram_with_native<S: EncodeLabelSet>(
        &mut self,
        sum: f64,
        count: u64,
        buckets: &[(f64, u64)],
        exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
        _native: NativeHistogram<'_>,
    ) -> Result<(), std::fmt::Error> {
        self.encode_histogram(sum, count, buckets, exemplars)
    }

    fn write_name(&mut self, suffix: &str) -> Result<(), std::fmt::Error> {
        if let Some(prefix) = self.prefix {
            self.writer.write_str(prefix.as_str())?;
            self.writer.write_str("_")?;
        }
        self.writer.write_str(self.name)?;
        if let Some(unit) = self.unit {
            self.writer.write_str("_")?;
            self.writer.write_str(unit.as_str())?;
        }
        match self.metric_type {
            MetricType::Counter => self.writer.write_str("_total")?,
            MetricType::Info => self.writer.write_str("_info")?,
            MetricType::Gauge | MetricType::Histogram | MetricType::Unknown => {}
        }
        self.writer.write_str(suffix)
    }

    fn write_labels<S: EncodeLabelSet>(
        &mut self,
        additional_labels: Option<&S>,
    ) -> Result<(), std::fmt::Error> {
        let mut writer = LabelWriter {
            writer: self.writer,
            opened: false,
            pending: true,
        };

        self.const_labels
            .encode(&mut text::LabelSetEncoder::new(&mut writer).into())?;

        if let Some(labels) = self.family_labels {
            writer.pending = true;
            labels.encode(&mut text::LabelSetEncoder::new(&mut writer).into())?;
        }

        if let Some(labels) = additional_labels {
            writer.pending = true;
            labels.encode(&mut text::LabelSetEncoder::new(&mut writer).into())?;
        }

        if writer.opened {
            writer.writer.write_str("}")?;
        }

        Ok(())
    }

    fn write_u64(&mut self, v: u64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(v))?;
        self.writer.write_str("\n")
    }
}

struct LabelWriter<'a> {
    writer: &'a mut dyn Write,
    opened: bool,
    pending: bool,
}

impl Write for LabelWriter<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if s.is_empty() {
            return Ok(());
        }
        if self.pending {
            self.writer.write_str(if self.opened { "," } else { "{" })?;
            self.opened = true;
            self.pending = false;
        }
        self.writer.write_str(s)
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
        self.writer
            .write_str(format_float(&mut dtoa::Buffer::new(), v))
    }

    pub fn encode_u64(&mut self, v: u64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(v))
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
        self.writer.write_str(itoa::Buffer::new().format(v))
    }

    pub fn encode_u64(&mut self, v: u64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(v))
    }

    pub fn encode_i64(&mut self, v: i64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(v))
    }

    pub fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer
            .write_str(format_float(&mut dtoa::Buffer::new(), v))
    }
}

fn format_float(buf: &mut dtoa::Buffer, value: f64) -> &str {
    if value.is_finite() {
        buf.format_finite(value)
    } else {
        format_nonfinite(value)
    }
}

#[cold]
fn format_nonfinite(value: f64) -> &'static str {
    if value.is_nan() {
        "NaN"
    } else if value.is_sign_positive() {
        "+Inf"
    } else {
        "-Inf"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collector::Collector;
    use crate::encoding::{DescriptorEncoder as GenericDescriptorEncoder, EncodeMetric};
    use crate::metrics::counter::{ConstCounter, Counter};
    use crate::metrics::exemplar::{CounterWithExemplar, HistogramWithExemplars};
    use crate::metrics::family::Family;
    use crate::metrics::gauge::Gauge;
    use crate::metrics::histogram::{Histogram, NativeHistogramConfig};
    use crate::metrics::info::Info;
    use pyo3::{prelude::*, types::PyModule};
    use std::sync::atomic::{AtomicI64, AtomicU32, AtomicU64};

    fn encode_to_string(registry: &Registry) -> String {
        let mut encoded = String::new();
        encode(&mut encoded, registry).unwrap();
        encoded
    }

    #[test]
    fn encode_counter() {
        let mut registry = Registry::default();
        let counter: Counter = Counter::default();
        registry.register("my_counter", "My counter", counter.clone());
        counter.inc_by(1_000_000);

        let float_counter: Counter<f64> = Counter::default();
        registry.register(
            "my_float_counter",
            "My float counter",
            float_counter.clone(),
        );
        float_counter.inc_by(1.5);

        let expected = "# HELP my_counter_total My counter.\n".to_owned()
            + "# TYPE my_counter_total counter\n"
            + "my_counter_total 1000000\n"
            + "# HELP my_float_counter_total My float counter.\n"
            + "# TYPE my_float_counter_total counter\n"
            + "my_float_counter_total 1.5\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_counter_with_unit_and_exemplar() {
        let mut registry = Registry::default();
        let counter: CounterWithExemplar<Vec<(String, u64)>> = CounterWithExemplar::default();
        registry.register_with_unit(
            "my_counter",
            "My counter",
            crate::registry::Unit::Seconds,
            counter.clone(),
        );
        counter.inc_by(1, Some(vec![("user_id".to_string(), 42)]), None);

        let expected = "# HELP my_counter_seconds_total My counter.\n".to_owned()
            + "# TYPE my_counter_seconds_total counter\n"
            + "my_counter_seconds_total 1\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_gauges() {
        let mut registry = Registry::default();
        let gauge: Gauge = Gauge::default();
        registry.register("my_gauge", "My gauge", gauge.clone());
        gauge.set(-3);

        let float_gauge = Gauge::<f64, std::sync::atomic::AtomicU64>::default();
        registry.register("my_float_gauge", "My float gauge", float_gauge.clone());
        float_gauge.set(0.25);

        let u32_gauge = Gauge::<u32, AtomicU32>::default();
        registry.register("my_u32_gauge", "My u32 gauge", u32_gauge.clone());
        u32_gauge.set(7);

        let u64_gauge = Gauge::<u64, AtomicU64>::default();
        registry.register("my_u64_gauge", "My u64 gauge", u64_gauge.clone());
        u64_gauge.set(0);

        let i64_gauge = Gauge::<i64, AtomicI64>::default();
        registry.register("my_i64_gauge", "My i64 gauge", i64_gauge.clone());
        i64_gauge.set(-1);

        let expected = "# HELP my_gauge My gauge.\n".to_owned()
            + "# TYPE my_gauge gauge\n"
            + "my_gauge -3\n"
            + "# HELP my_float_gauge My float gauge.\n"
            + "# TYPE my_float_gauge gauge\n"
            + "my_float_gauge 0.25\n"
            + "# HELP my_u32_gauge My u32 gauge.\n"
            + "# TYPE my_u32_gauge gauge\n"
            + "my_u32_gauge 7\n"
            + "# HELP my_u64_gauge My u64 gauge.\n"
            + "# TYPE my_u64_gauge gauge\n"
            + "my_u64_gauge 0\n"
            + "# HELP my_i64_gauge My i64 gauge.\n"
            + "# TYPE my_i64_gauge gauge\n"
            + "my_i64_gauge -1\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_info() {
        let mut registry =
            Registry::with_labels(vec![(Cow::Borrowed("env"), Cow::Borrowed("prod"))].into_iter());
        let info = Info::new(vec![("os".to_string(), "GNU/linux".to_string())]);
        registry.register("my_info_metric", "My info metric", info);

        let expected = "# HELP my_info_metric_info My info metric.\n".to_owned()
            + "# TYPE my_info_metric_info gauge\n"
            + "my_info_metric_info{env=\"prod\",os=\"GNU/linux\"} 1\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_unknown_collector() {
        #[derive(Debug)]
        struct MyCollector;

        impl Collector for MyCollector {
            fn encode(&self, mut encoder: GenericDescriptorEncoder) -> Result<(), std::fmt::Error> {
                let counter = ConstCounter::new(42u64);
                let metric_encoder = encoder.encode_descriptor(
                    "my_const_counter",
                    "some help",
                    None,
                    counter.metric_type(),
                )?;
                counter.encode(metric_encoder)?;

                let metric_encoder = encoder.encode_descriptor(
                    "my_untyped",
                    "untyped help",
                    None,
                    MetricType::Unknown,
                )?;
                ConstCounter::new(3u64).encode(metric_encoder)?;
                Ok(())
            }
        }

        let mut registry = Registry::default();
        registry.register_collector(Box::new(MyCollector));

        let expected = "# HELP my_const_counter_total some help\n".to_owned()
            + "# TYPE my_const_counter_total counter\n"
            + "my_const_counter_total 42\n"
            + "# HELP my_untyped untyped help\n"
            + "# TYPE my_untyped untyped\n"
            + "my_untyped 3\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_special_values() {
        let mut registry = Registry::default();
        let pos_inf = Gauge::<f64, AtomicU64>::default();
        registry.register("pos_inf", "Pos inf", pos_inf.clone());
        pos_inf.set(f64::INFINITY);

        let neg_inf = Gauge::<f64, AtomicU64>::default();
        registry.register("neg_inf", "Neg inf", neg_inf.clone());
        neg_inf.set(f64::NEG_INFINITY);

        let nan = Gauge::<f64, AtomicU64>::default();
        registry.register("nan", "NaN", nan.clone());
        nan.set(f64::NAN);

        let float_counter: Counter<f64> = Counter::default();
        registry.register("inf_counter", "Inf counter", float_counter.clone());
        float_counter.inc_by(f64::INFINITY);

        let big: Counter = Counter::default();
        registry.register("big", "Big", big.clone());
        big.inc_by(9007199254740993);

        let expected = "# HELP pos_inf Pos inf.\n".to_owned()
            + "# TYPE pos_inf gauge\n"
            + "pos_inf +Inf\n"
            + "# HELP neg_inf Neg inf.\n"
            + "# TYPE neg_inf gauge\n"
            + "neg_inf -Inf\n"
            + "# HELP nan NaN.\n"
            + "# TYPE nan gauge\n"
            + "nan NaN\n"
            + "# HELP inf_counter_total Inf counter.\n"
            + "# TYPE inf_counter_total counter\n"
            + "inf_counter_total +Inf\n"
            + "# HELP big_total Big.\n"
            + "# TYPE big_total counter\n"
            + "big_total 9007199254740993\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_histogram() {
        let mut registry = Registry::default();
        let histogram = Histogram::new([1.0, 2.5, 1e6]);
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);
        histogram.observe(2_000_000.0);

        let expected = "# HELP my_histogram My histogram.\n".to_owned()
            + "# TYPE my_histogram histogram\n"
            + "my_histogram_bucket{le=\"1.0\"} 1\n"
            + "my_histogram_bucket{le=\"2.5\"} 1\n"
            + "my_histogram_bucket{le=\"1000000.0\"} 1\n"
            + "my_histogram_bucket{le=\"+Inf\"} 2\n"
            + "my_histogram_sum 2000001.0\n"
            + "my_histogram_count 2\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    type HistogramFamily =
        Family<Vec<(String, String)>, HistogramWithExemplars<Vec<(String, u64)>>>;

    fn register_histogram_family(registry: &mut Registry) -> HistogramFamily {
        let family = HistogramFamily::new_with_constructor(|| {
            HistogramWithExemplars::new([0.5].into_iter())
        });
        registry.register("my_histogram_family", "My histogram family", family.clone());
        family
    }

    fn get_label_set() -> Vec<(String, String)> {
        vec![("method".to_string(), "GET".to_string())]
    }

    fn post_500_label_set() -> Vec<(String, String)> {
        vec![
            ("method".to_string(), "POST".to_string()),
            ("status".to_string(), "500".to_string()),
        ]
    }

    /// Asserts `encoded == prefix + <blocks in some order>`. Needed because
    /// `Family` iterates a `HashMap`, so the order of label sets is unspecified.
    fn assert_encoded_with_blocks_in_any_order(encoded: &str, prefix: &str, blocks: &[&str]) {
        let mut rest = encoded
            .strip_prefix(prefix)
            .unwrap_or_else(|| panic!("expected prefix:\n{prefix}\ngot:\n{encoded}"))
            .to_owned();
        for block in blocks {
            assert!(
                rest.contains(block),
                "missing block:\n{block}\nin:\n{encoded}"
            );
            rest = rest.replacen(block, "", 1);
        }
        assert!(rest.is_empty(), "unexpected trailing output:\n{rest}");
    }

    #[test]
    fn encode_histogram_without_observations() {
        let mut registry = Registry::default();
        let histogram = HistogramWithExemplars::<Vec<(String, u64)>>::new([0.5].into_iter());
        registry.register("my_histogram", "My histogram", histogram.clone());

        let family = register_histogram_family(&mut registry);
        drop(family.get_or_create(&get_label_set()));
        drop(family.get_or_create(&post_500_label_set()));

        let prefix = "# HELP my_histogram My histogram.\n".to_owned()
            + "# TYPE my_histogram histogram\n"
            + "my_histogram_bucket{le=\"0.5\"} 0\n"
            + "my_histogram_bucket{le=\"+Inf\"} 0\n"
            + "my_histogram_sum 0.0\n"
            + "my_histogram_count 0\n"
            + "# HELP my_histogram_family My histogram family.\n"
            + "# TYPE my_histogram_family histogram\n";
        let get_block = "my_histogram_family_bucket{method=\"GET\",le=\"0.5\"} 0\n".to_owned()
            + "my_histogram_family_bucket{method=\"GET\",le=\"+Inf\"} 0\n"
            + "my_histogram_family_sum{method=\"GET\"} 0.0\n"
            + "my_histogram_family_count{method=\"GET\"} 0\n";
        let post_block = "my_histogram_family_bucket{method=\"POST\",status=\"500\",le=\"0.5\"} 0\n"
            .to_owned()
            + "my_histogram_family_bucket{method=\"POST\",status=\"500\",le=\"+Inf\"} 0\n"
            + "my_histogram_family_sum{method=\"POST\",status=\"500\"} 0.0\n"
            + "my_histogram_family_count{method=\"POST\",status=\"500\"} 0\n";

        let encoded = encode_to_string(&registry);
        assert_encoded_with_blocks_in_any_order(&encoded, &prefix, &[&get_block, &post_block]);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_histogram_drops_exemplars() {
        let mut registry = Registry::default();
        let histogram = HistogramWithExemplars::new([0.5].into_iter());
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(0.25, Some(vec![("user_id".to_string(), 42u64)]), None);

        let family = register_histogram_family(&mut registry);
        family.get_or_create(&get_label_set()).observe(
            0.25,
            Some(vec![("user_id".to_string(), 42)]),
            None,
        );
        family.get_or_create(&post_500_label_set()).observe(
            0.75,
            Some(vec![("user_id".to_string(), 7)]),
            None,
        );

        let prefix = "# HELP my_histogram My histogram.\n".to_owned()
            + "# TYPE my_histogram histogram\n"
            + "my_histogram_bucket{le=\"0.5\"} 1\n"
            + "my_histogram_bucket{le=\"+Inf\"} 1\n"
            + "my_histogram_sum 0.25\n"
            + "my_histogram_count 1\n"
            + "# HELP my_histogram_family My histogram family.\n"
            + "# TYPE my_histogram_family histogram\n";
        let get_block = "my_histogram_family_bucket{method=\"GET\",le=\"0.5\"} 1\n".to_owned()
            + "my_histogram_family_bucket{method=\"GET\",le=\"+Inf\"} 1\n"
            + "my_histogram_family_sum{method=\"GET\"} 0.25\n"
            + "my_histogram_family_count{method=\"GET\"} 1\n";
        let post_block = "my_histogram_family_bucket{method=\"POST\",status=\"500\",le=\"0.5\"} 0\n"
            .to_owned()
            + "my_histogram_family_bucket{method=\"POST\",status=\"500\",le=\"+Inf\"} 1\n"
            + "my_histogram_family_sum{method=\"POST\",status=\"500\"} 0.75\n"
            + "my_histogram_family_count{method=\"POST\",status=\"500\"} 1\n";

        let encoded = encode_to_string(&registry);
        assert_encoded_with_blocks_in_any_order(&encoded, &prefix, &[&get_block, &post_block]);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_native_histograms() {
        let mut registry = Registry::default();
        let native_only = Histogram::new_native(NativeHistogramConfig::default());
        registry.register("native_only", "Native only", native_only.clone());
        native_only.observe(3.0);

        let native_and_classic =
            Histogram::new_classic_and_native([1.0], NativeHistogramConfig::default());
        registry.register(
            "native_and_classic",
            "Native and classic",
            native_and_classic.clone(),
        );
        native_and_classic.observe(0.5);

        let expected = "# HELP native_only Native only.\n".to_owned()
            + "# TYPE native_only histogram\n"
            + "native_only_bucket{le=\"+Inf\"} 1\n"
            + "native_only_sum 3.0\n"
            + "native_only_count 1\n"
            + "# HELP native_and_classic Native and classic.\n"
            + "# TYPE native_and_classic histogram\n"
            + "native_and_classic_bucket{le=\"1.0\"} 1\n"
            + "native_and_classic_bucket{le=\"+Inf\"} 1\n"
            + "native_and_classic_sum 0.5\n"
            + "native_and_classic_count 1\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_family_with_const_labels_and_prefix() {
        let mut registry = Registry::default();
        let sub_registry = registry
            .sub_registry_with_prefix("my_prefix")
            .sub_registry_with_label((Cow::Borrowed("region"), Cow::Borrowed("eu")));

        let counters = Family::<Vec<(String, String)>, Counter>::default();
        sub_registry.register("my_counter_family", "My counter family", counters.clone());
        counters
            .get_or_create(&vec![
                ("method".to_string(), "GET".to_string()),
                ("status".to_string(), "200".to_string()),
            ])
            .inc();

        let histograms = Family::<Vec<(String, String)>, Histogram>::new_with_constructor(|| {
            Histogram::new([1.0])
        });
        sub_registry.register(
            "my_histogram_family",
            "My histogram family",
            histograms.clone(),
        );
        histograms
            .get_or_create(&vec![("method".to_string(), "GET".to_string())])
            .observe(0.5);

        let expected = "# HELP my_prefix_my_counter_family_total My counter family.\n".to_owned()
            + "# TYPE my_prefix_my_counter_family_total counter\n"
            + "my_prefix_my_counter_family_total{region=\"eu\",method=\"GET\",status=\"200\"} 1\n"
            + "# HELP my_prefix_my_histogram_family My histogram family.\n"
            + "# TYPE my_prefix_my_histogram_family histogram\n"
            + "my_prefix_my_histogram_family_bucket{region=\"eu\",method=\"GET\",le=\"1.0\"} 1\n"
            + "my_prefix_my_histogram_family_bucket{region=\"eu\",method=\"GET\",le=\"+Inf\"} 1\n"
            + "my_prefix_my_histogram_family_sum{region=\"eu\",method=\"GET\"} 0.5\n"
            + "my_prefix_my_histogram_family_count{region=\"eu\",method=\"GET\"} 1\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_family_with_empty_label_set() {
        let mut registry = Registry::default();
        let family = Family::<Vec<(String, String)>, Counter>::default();
        registry.register("my_counter", "My counter", family.clone());
        family.get_or_create(&vec![]).inc();

        let expected = "# HELP my_counter_total My counter.\n".to_owned()
            + "# TYPE my_counter_total counter\n"
            + "my_counter_total 1\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    #[test]
    fn encode_omits_empty_family() {
        let mut registry = Registry::default();
        let family = Family::<Vec<(String, String)>, Counter>::default();
        registry.register("my_counter", "My counter", family);

        assert_eq!("", encode_to_string(&registry));
    }

    #[test]
    fn encode_escapes_help_and_label_values() {
        let mut registry = Registry::default();
        let family = Family::<Vec<(String, String)>, Gauge>::default();
        registry.register(
            "my_gauge",
            "Help with \\ backslash, \"quotes\" and\nnewline",
            family.clone(),
        );
        family
            .get_or_create(&vec![(
                "path".to_string(),
                "C:\\dir \"quoted\"\nnext".to_string(),
            )])
            .set(2);

        let expected = "# HELP my_gauge Help with \\\\ backslash, \"quotes\" and\\nnewline.\n"
            .to_owned()
            + "# TYPE my_gauge gauge\n"
            + "my_gauge{path=\"C:\\\\dir \\\"quoted\\\"\\nnext\"} 2\n";
        let encoded = encode_to_string(&registry);
        assert_eq!(expected, encoded);
        parse_with_python_client(encoded);
    }

    fn parse_with_python_client(input: String) {
        pyo3::Python::initialize();

        println!("{input:?}");
        Python::attach(|py| {
            let parser = PyModule::from_code(
                py,
                c"
from prometheus_client.parser import text_string_to_metric_families

def parse(input):
    families = text_string_to_metric_families(input)
    list(families)
",
                c"parser.py",
                c"parser",
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
