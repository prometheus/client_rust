//! Open Metrics text format implementation.
//!
//! ```
//! # use prometheus_client::encoding::text::encode;
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
//! encode(&mut buffer, &registry).unwrap();
//!
//! let expected = "# HELP my_counter This is my counter.\n".to_owned() +
//!                "# TYPE my_counter counter\n" +
//!                "my_counter_total 1\n" +
//!                "# EOF\n";
//! assert_eq!(expected, buffer);
//! ```

use crate::encoding::{EncodeExemplarValue, EncodeLabelSet, EncodeMetric};
use crate::metrics::exemplar::Exemplar;
use crate::registry::{Descriptor, Registry, Unit};

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write;

/// Encode the metrics registered with the provided [`Registry`] into the
/// provided [`Write`]r using the OpenMetrics text format.
pub fn encode<W>(writer: &mut W, registry: &Registry) -> Result<(), std::fmt::Error>
where
    W: Write,
{
    for (desc, metric) in registry.iter_metrics() {
        encode_metric(writer, desc, metric.as_ref())?;
    }
    for (desc, metric) in registry.iter_collectors() {
        encode_metric(writer, desc.as_ref(), metric.as_ref())?;
    }

    writer.write_str("# EOF\n")?;

    Ok(())
}

fn encode_metric<W>(
    writer: &mut W,
    desc: &Descriptor,
    metric: &(impl EncodeMetric + ?Sized),
) -> Result<(), std::fmt::Error>
where
    W: Write,
{
    writer.write_str("# HELP ")?;
    writer.write_str(desc.name())?;
    if let Some(unit) = desc.unit() {
        writer.write_str("_")?;
        writer.write_str(unit.as_str())?;
    }
    writer.write_str(" ")?;
    writer.write_str(desc.help())?;
    writer.write_str("\n")?;

    writer.write_str("# TYPE ")?;
    writer.write_str(desc.name())?;
    if let Some(unit) = desc.unit() {
        writer.write_str("_")?;
        writer.write_str(unit.as_str())?;
    }
    writer.write_str(" ")?;
    writer.write_str(EncodeMetric::metric_type(metric).as_str())?;
    writer.write_str("\n")?;

    if let Some(unit) = desc.unit() {
        writer.write_str("# UNIT ")?;
        writer.write_str(desc.name())?;
        writer.write_str("_")?;
        writer.write_str(unit.as_str())?;
        writer.write_str(" ")?;
        writer.write_str(unit.as_str())?;
        writer.write_str("\n")?;
    }

    let encoder = MetricEncoder {
        writer,
        name: desc.name(),
        unit: desc.unit(),
        const_labels: desc.labels(),
        family_labels: None,
    }
    .into();

    EncodeMetric::encode(metric, encoder)?;

    Ok(())
}

/// Helper type for [`EncodeMetric`], see [`EncodeMetric::encode`].
pub(crate) struct MetricEncoder<'a, 'b> {
    writer: &'a mut dyn Write,
    name: &'a str,
    unit: &'a Option<Unit>,
    const_labels: &'a [(Cow<'static, str>, Cow<'static, str>)],
    family_labels: Option<&'b dyn super::EncodeLabelSet>,
}

impl<'a, 'b> std::fmt::Debug for MetricEncoder<'a, 'b> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut labels = String::new();
        if let Some(l) = self.family_labels {
            l.encode(LabelSetEncoder::new(&mut labels).into())?;
        }

        f.debug_struct("Encoder")
            .field("name", &self.name)
            .field("unit", &self.unit)
            .field("const_labels", &self.const_labels)
            .field("labels", &labels.as_str())
            .finish()
    }
}

impl<'a, 'b> MetricEncoder<'a, 'b> {
    pub fn encode_counter<
        S: EncodeLabelSet,
        CounterValue: super::EncodeCounterValue,
        ExemplarValue: EncodeExemplarValue,
    >(
        &mut self,
        v: &CounterValue,
        exemplar: Option<&Exemplar<S, ExemplarValue>>,
    ) -> Result<(), std::fmt::Error> {
        self.write_name_and_unit()?;

        self.write_suffix("total")?;

        self.encode_labels::<()>(None)?;

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
        self.write_name_and_unit()?;

        self.encode_labels::<()>(None)?;

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
        self.write_name_and_unit()?;

        self.write_suffix("info")?;

        self.encode_labels(Some(label_set))?;

        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(1))?;

        self.newline()?;

        Ok(())
    }

    /// Encode a set of labels. Used by wrapper metric types like
    /// [`Family`](crate::metrics::family::Family).
    pub fn encode_family<'c, 'd, S: EncodeLabelSet>(
        &'c mut self,
        label_set: &'d S,
    ) -> Result<MetricEncoder<'c, 'd>, std::fmt::Error> {
        debug_assert!(self.family_labels.is_none());

        Ok(MetricEncoder {
            writer: self.writer,
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
        self.write_name_and_unit()?;
        self.write_suffix("sum")?;
        self.encode_labels::<()>(None)?;
        self.writer.write_str(" ")?;
        self.writer.write_str(dtoa::Buffer::new().format(sum))?;
        self.newline()?;

        self.write_name_and_unit()?;
        self.write_suffix("count")?;
        self.encode_labels::<()>(None)?;
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(count))?;
        self.newline()?;

        let mut cummulative = 0;
        for (i, (upper_bound, count)) in buckets.iter().enumerate() {
            cummulative += count;

            self.write_name_and_unit()?;
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
    fn write_name_and_unit(&mut self) -> Result<(), std::fmt::Error> {
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

        if let Some(labels) = &self.family_labels {
            if !self.const_labels.is_empty() || additional_labels.is_some() {
                self.writer.write_str(",")?;
            }

            labels.encode(LabelSetEncoder::new(self.writer).into())?;
        }

        self.writer.write_str("}")?;

        Ok(())
    }
}

pub(crate) struct CounterValueEncoder<'a> {
    writer: &'a mut dyn Write,
}

impl<'a> std::fmt::Debug for CounterValueEncoder<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CounterValueEncoder").finish()
    }
}

impl<'a> CounterValueEncoder<'a> {
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

impl<'a> std::fmt::Debug for GaugeValueEncoder<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GaugeValueEncoder").finish()
    }
}

impl<'a> GaugeValueEncoder<'a> {
    pub fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(dtoa::Buffer::new().format(v))?;
        Ok(())
    }

    pub fn encode_i64(&mut self, v: i64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(" ")?;
        self.writer.write_str(itoa::Buffer::new().format(v))?;
        Ok(())
    }
}

pub(crate) struct ExemplarValueEncoder<'a> {
    writer: &'a mut dyn Write,
}

impl<'a> std::fmt::Debug for ExemplarValueEncoder<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExemplarValueEncoder").finish()
    }
}

impl<'a> ExemplarValueEncoder<'a> {
    pub fn encode(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        self.writer.write_str(dtoa::Buffer::new().format(v))
    }
}

pub(crate) struct LabelSetEncoder<'a> {
    writer: &'a mut dyn Write,
    first: bool,
}

impl<'a> std::fmt::Debug for LabelSetEncoder<'a> {
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

impl<'a> std::fmt::Debug for LabelEncoder<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelEncoder")
            .field("first", &self.first)
            .finish()
    }
}

impl<'a> LabelEncoder<'a> {
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

impl<'a> std::fmt::Debug for LabelKeyEncoder<'a> {
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

impl<'a> std::fmt::Write for LabelKeyEncoder<'a> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.writer.write_str(s)
    }
}

pub(crate) struct LabelValueEncoder<'a> {
    writer: &'a mut dyn Write,
}

impl<'a> std::fmt::Debug for LabelValueEncoder<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LabelValueEncoder").finish()
    }
}

impl<'a> LabelValueEncoder<'a> {
    pub fn finish(self) -> Result<(), std::fmt::Error> {
        self.writer.write_str("\"")
    }
}

impl<'a> std::fmt::Write for LabelValueEncoder<'a> {
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

    #[test]
    fn encode_counter() {
        let counter: Counter = Counter::default();
        let mut registry = Registry::default();
        registry.register("my_counter", "My counter", counter);

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

    fn parse_with_python_client(input: String) {
        pyo3::prepare_freethreaded_python();

        println!("{:?}", input);
        Python::with_gil(|py| {
            let parser = PyModule::from_code(
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
