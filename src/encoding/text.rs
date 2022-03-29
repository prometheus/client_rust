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
//! let mut buffer = vec![];
//! encode(&mut buffer, &registry).unwrap();
//!
//! let expected = "# HELP my_counter This is my counter.\n".to_owned() +
//!                "# TYPE my_counter counter\n" +
//!                "my_counter_total 1\n" +
//!                "# EOF\n";
//! assert_eq!(expected, String::from_utf8(buffer).unwrap());
//! ```

use crate::metrics::counter::{self, Counter};
use crate::metrics::exemplar::{CounterWithExemplar, Exemplar, HistogramWithExemplars};
use crate::metrics::family::{Family, MetricConstructor};
use crate::metrics::gauge::{self, Gauge};
use crate::metrics::histogram::Histogram;
use crate::metrics::info::Info;
use crate::metrics::{MetricType, TypedMetric};
use crate::registry::{Registry, Unit};

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::Write;
use std::ops::Deref;

pub use prometheus_client_derive_text_encode::*;

pub fn encode<W, M>(writer: &mut W, registry: &Registry<M>) -> Result<(), std::io::Error>
where
    W: Write,
    M: EncodeMetric,
{
    for (desc, metric) in registry.iter() {
        writer.write_all(b"# HELP ")?;
        writer.write_all(desc.name().as_bytes())?;
        if let Some(unit) = desc.unit() {
            writer.write_all(b"_")?;
            unit.encode(writer)?;
        }
        writer.write_all(b" ")?;
        writer.write_all(desc.help().as_bytes())?;
        writer.write_all(b"\n")?;

        writer.write_all(b"# TYPE ")?;
        writer.write_all(desc.name().as_bytes())?;
        if let Some(unit) = desc.unit() {
            writer.write_all(b"_")?;
            unit.encode(writer)?;
        }
        writer.write_all(b" ")?;
        metric.metric_type().encode(writer)?;
        writer.write_all(b"\n")?;

        if let Some(unit) = desc.unit() {
            writer.write_all(b"# UNIT ")?;
            writer.write_all(desc.name().as_bytes())?;
            writer.write_all(b"_")?;
            unit.encode(writer)?;
            writer.write_all(b" ")?;
            unit.encode(writer)?;
            writer.write_all(b"\n")?;
        }

        let encoder = Encoder {
            writer,
            name: desc.name(),
            unit: desc.unit(),
            const_labels: desc.labels(),
            labels: None,
        };

        metric.encode(encoder)?;
    }

    writer.write_all(b"# EOF\n")?;

    Ok(())
}

pub trait Encode {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error>;
}

impl Encode for f64 {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        writer.write_all(dtoa::Buffer::new().format(*self).as_bytes())?;
        Ok(())
    }
}

impl Encode for u64 {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        writer.write_all(itoa::Buffer::new().format(*self).as_bytes())?;
        Ok(())
    }
}

impl Encode for u32 {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        writer.write_all(itoa::Buffer::new().format(*self).as_bytes())?;
        Ok(())
    }
}

impl<T: Encode> Encode for &[T] {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        if self.is_empty() {
            return Ok(());
        }

        let mut iter = self.iter().peekable();
        while let Some(x) = iter.next() {
            x.encode(writer)?;

            if iter.peek().is_some() {
                writer.write_all(b",")?;
            }
        }

        Ok(())
    }
}

impl<T: Encode> Encode for Vec<T> {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        self.as_slice().encode(writer)
    }
}

impl<K: Encode, V: Encode> Encode for (K, V) {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        let (key, value) = self;

        key.encode(writer)?;
        writer.write_all(b"=\"")?;

        value.encode(writer)?;
        writer.write_all(b"\"")?;

        Ok(())
    }
}

impl Encode for &str {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        // TODO: Can we do better?
        writer.write_all(self.as_bytes())?;
        Ok(())
    }
}

impl Encode for String {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        self.as_str().encode(writer)
    }
}

impl<'a> Encode for Cow<'a, str> {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        self.as_ref().encode(writer)
    }
}

impl Encode for MetricType {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        let t = match self {
            MetricType::Counter => "counter",
            MetricType::Gauge => "gauge",
            MetricType::Histogram => "histogram",
            MetricType::Info => "info",
            MetricType::Unknown => "unknown",
        };

        writer.write_all(t.as_bytes())?;
        Ok(())
    }
}

impl Encode for Unit {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        let u = match self {
            Unit::Amperes => "amperes",
            Unit::Bytes => "bytes",
            Unit::Celsius => "celsius",
            Unit::Grams => "grams",
            Unit::Joules => "joules",
            Unit::Meters => "meters",
            Unit::Ratios => "ratios",
            Unit::Seconds => "seconds",
            Unit::Volts => "volts",
            Unit::Other(other) => other.as_str(),
        };

        writer.write_all(u.as_bytes())?;
        Ok(())
    }
}

impl Encode for () {
    fn encode(&self, _writer: &mut dyn Write) -> Result<(), std::io::Error> {
        Ok(())
    }
}

/// Helper type for [`EncodeMetric`], see [`EncodeMetric::encode`].
///
// `Encoder` does not take a trait parameter for `writer` and `labels` because
// `EncodeMetric` which uses `Encoder` needs to be usable as a trait object in
// order to be able to register different metric types with a `Registry`. Trait
// objects can not use type parameters.
//
// TODO: Alternative solutions to the above are very much appreciated.
pub struct Encoder<'a, 'b> {
    writer: &'a mut dyn Write,
    name: &'a str,
    unit: &'a Option<Unit>,
    const_labels: &'a [(Cow<'static, str>, Cow<'static, str>)],
    labels: Option<&'b dyn Encode>,
}

impl<'a, 'b> Encoder<'a, 'b> {
    /// Encode a metric suffix, e.g. in the case of [`Counter`] the suffic `_total`.
    pub fn encode_suffix(&mut self, suffix: &'static str) -> Result<BucketEncoder, std::io::Error> {
        self.write_name_and_unit()?;

        self.writer.write_all(b"_")?;
        self.writer.write_all(suffix.as_bytes()).map(|_| ())?;

        self.encode_labels()
    }

    /// Signal that the metric has no suffix.
    pub fn no_suffix(&mut self) -> Result<BucketEncoder, std::io::Error> {
        self.write_name_and_unit()?;

        self.encode_labels()
    }

    fn write_name_and_unit(&mut self) -> Result<(), std::io::Error> {
        self.writer.write_all(self.name.as_bytes())?;
        if let Some(unit) = self.unit {
            self.writer.write_all(b"_")?;
            unit.encode(self.writer)?;
        }

        Ok(())
    }

    // TODO: Consider caching the encoded labels for Histograms as they stay the
    // same but are currently encoded multiple times.
    fn encode_labels(&mut self) -> Result<BucketEncoder, std::io::Error> {
        let mut opened_curly_brackets = false;

        if !self.const_labels.is_empty() {
            self.writer.write_all(b"{")?;
            opened_curly_brackets = true;

            self.const_labels.encode(self.writer)?;
        }

        if let Some(labels) = &self.labels {
            if opened_curly_brackets {
                self.writer.write_all(b",")?;
            } else {
                opened_curly_brackets = true;
                self.writer.write_all(b"{")?;
            }
            labels.encode(self.writer)?;
        }

        Ok(BucketEncoder {
            opened_curly_brackets,
            writer: self.writer,
        })
    }

    /// Encode a set of labels. Used by wrapper metric types like [`Family`].
    pub fn with_label_set<'c, 'd>(&'c mut self, label_set: &'d dyn Encode) -> Encoder<'c, 'd> {
        debug_assert!(self.labels.is_none());

        Encoder {
            writer: self.writer,
            name: self.name,
            unit: self.unit,
            const_labels: self.const_labels,
            labels: Some(label_set),
        }
    }
}

#[must_use]
pub struct BucketEncoder<'a> {
    writer: &'a mut dyn Write,
    opened_curly_brackets: bool,
}

impl<'a> BucketEncoder<'a> {
    /// Encode a bucket. Used for the [`Histogram`] metric type.
    pub fn encode_bucket(&mut self, upper_bound: f64) -> Result<ValueEncoder, std::io::Error> {
        if self.opened_curly_brackets {
            self.writer.write_all(b",")?;
        } else {
            self.writer.write_all(b"{")?;
        }

        self.writer.write_all(b"le=\"")?;
        if upper_bound == f64::MAX {
            self.writer.write_all(b"+Inf")?;
        } else {
            upper_bound.encode(self.writer)?;
        }
        self.writer.write_all(b"\"}")?;

        Ok(ValueEncoder {
            writer: self.writer,
        })
    }

    /// Signal that the metric type has no bucket.
    pub fn no_bucket(&mut self) -> Result<ValueEncoder, std::io::Error> {
        if self.opened_curly_brackets {
            self.writer.write_all(b"}")?;
        }
        Ok(ValueEncoder {
            writer: self.writer,
        })
    }
}

#[must_use]
pub struct ValueEncoder<'a> {
    writer: &'a mut dyn Write,
}

impl<'a> ValueEncoder<'a> {
    /// Encode the metric value. E.g. in the case of [`Counter`] the
    /// monotonically increasing counter value.
    pub fn encode_value<V: Encode>(&mut self, v: V) -> Result<ExemplarEncoder, std::io::Error> {
        self.writer.write_all(b" ")?;
        v.encode(self.writer)?;
        Ok(ExemplarEncoder {
            writer: self.writer,
        })
    }
}

#[must_use]
pub struct ExemplarEncoder<'a> {
    writer: &'a mut dyn Write,
}

impl<'a> ExemplarEncoder<'a> {
    /// Encode an exemplar for the given metric.
    pub fn encode_exemplar<S: Encode, V: Encode>(
        &mut self,
        exemplar: &Exemplar<S, V>,
    ) -> Result<(), std::io::Error> {
        self.writer.write_all(b" # {")?;
        exemplar.label_set.encode(self.writer)?;
        self.writer.write_all(b"} ")?;
        exemplar.value.encode(self.writer)?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }

    /// Signal that the metric type has no exemplar.
    pub fn no_exemplar(&mut self) -> Result<(), std::io::Error> {
        self.writer.write_all(b"\n")?;
        Ok(())
    }
}

/// Trait implemented by each metric type, e.g. [`Counter`], to implement its encoding.
pub trait EncodeMetric {
    fn encode(&self, encoder: Encoder) -> Result<(), std::io::Error>;

    // One can not use [`TypedMetric`] directly, as associated constants are not
    // object safe and thus can not be used with dynamic dispatching.
    fn metric_type(&self) -> MetricType;
}

impl EncodeMetric for Box<dyn EncodeMetric> {
    fn encode(&self, encoder: Encoder) -> Result<(), std::io::Error> {
        self.deref().encode(encoder)
    }

    fn metric_type(&self) -> MetricType {
        self.deref().metric_type()
    }
}

pub trait SendSyncEncodeMetric: EncodeMetric + Send + Sync {}

impl<T: EncodeMetric + Send + Sync> SendSyncEncodeMetric for T {}

impl EncodeMetric for Box<dyn SendSyncEncodeMetric> {
    fn encode(&self, encoder: Encoder) -> Result<(), std::io::Error> {
        self.deref().encode(encoder)
    }

    fn metric_type(&self) -> MetricType {
        self.deref().metric_type()
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Counter

impl<N, A> EncodeMetric for Counter<N, A>
where
    N: Encode,
    A: counter::Atomic<N>,
{
    fn encode(&self, encoder: Encoder) -> Result<(), std::io::Error> {
        // TODO: Would be better to use never type instead of `()`.
        encode_counter_with_maybe_exemplar::<(), _>(self.get(), None, encoder)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

// TODO: S, V, N, A are hard to grasp.
impl<S, N, A> EncodeMetric for CounterWithExemplar<S, N, A>
where
    S: Encode,
    N: Encode + Clone,
    A: counter::Atomic<N>,
{
    fn encode(&self, encoder: Encoder) -> Result<(), std::io::Error> {
        let (value, exemplar) = self.get();
        encode_counter_with_maybe_exemplar(value, exemplar.as_ref().as_ref(), encoder)
    }

    fn metric_type(&self) -> MetricType {
        Counter::<N, A>::TYPE
    }
}

fn encode_counter_with_maybe_exemplar<S, N>(
    value: N,
    exemplar: Option<&Exemplar<S, N>>,
    mut encoder: Encoder,
) -> Result<(), std::io::Error>
where
    S: Encode,
    N: Encode,
{
    let mut bucket_encoder = encoder.encode_suffix("total")?;
    let mut value_encoder = bucket_encoder.no_bucket()?;
    let mut exemplar_encoder = value_encoder.encode_value(value)?;

    match exemplar {
        Some(exemplar) => exemplar_encoder.encode_exemplar(exemplar)?,
        None => exemplar_encoder.no_exemplar()?,
    }

    Ok(())
}

/////////////////////////////////////////////////////////////////////////////////
// Gauge

impl<N, A> EncodeMetric for Gauge<N, A>
where
    N: Encode,
    A: gauge::Atomic<N>,
{
    fn encode(&self, mut encoder: Encoder) -> Result<(), std::io::Error> {
        encoder
            .no_suffix()?
            .no_bucket()?
            .encode_value(self.get())?
            .no_exemplar()?;

        Ok(())
    }
    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Family

impl<S, M, C> EncodeMetric for Family<S, M, C>
where
    S: Clone + std::hash::Hash + Eq + Encode,
    M: EncodeMetric + TypedMetric,
    C: MetricConstructor<M>,
{
    fn encode(&self, mut encoder: Encoder) -> Result<(), std::io::Error> {
        let guard = self.read();
        for (label_set, m) in guard.iter() {
            let encoder = encoder.with_label_set(label_set);
            m.encode(encoder)?;
        }
        Ok(())
    }

    fn metric_type(&self) -> MetricType {
        M::TYPE
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Histogram

impl EncodeMetric for Histogram {
    fn encode(&self, encoder: Encoder) -> Result<(), std::io::Error> {
        let (sum, count, buckets) = self.get();
        // TODO: Would be better to use never type instead of `()`.
        encode_histogram_with_maybe_exemplars::<()>(sum, count, &buckets, None, encoder)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

impl<S: Encode> EncodeMetric for HistogramWithExemplars<S> {
    fn encode(&self, encoder: Encoder) -> Result<(), std::io::Error> {
        let inner = self.inner();
        let (sum, count, buckets) = inner.histogram.get();
        encode_histogram_with_maybe_exemplars(sum, count, &buckets, Some(&inner.exemplars), encoder)
    }

    fn metric_type(&self) -> MetricType {
        Histogram::TYPE
    }
}

fn encode_histogram_with_maybe_exemplars<S: Encode>(
    sum: f64,
    count: u64,
    buckets: &[(f64, u64)],
    exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
    mut encoder: Encoder,
) -> Result<(), std::io::Error> {
    encoder
        .encode_suffix("sum")?
        .no_bucket()?
        .encode_value(sum)?
        .no_exemplar()?;
    encoder
        .encode_suffix("count")?
        .no_bucket()?
        .encode_value(count)?
        .no_exemplar()?;

    let mut cummulative = 0;
    for (i, (upper_bound, count)) in buckets.iter().enumerate() {
        cummulative += count;
        let mut bucket_encoder = encoder.encode_suffix("bucket")?;
        let mut value_encoder = bucket_encoder.encode_bucket(*upper_bound)?;
        let mut exemplar_encoder = value_encoder.encode_value(cummulative)?;

        match exemplars.and_then(|es| es.get(&i)) {
            Some(exemplar) => exemplar_encoder.encode_exemplar(exemplar)?,
            None => exemplar_encoder.no_exemplar()?,
        }
    }

    Ok(())
}

/////////////////////////////////////////////////////////////////////////////////
// Info

impl<S> EncodeMetric for Info<S>
where
    S: Clone + std::hash::Hash + Eq + Encode,
{
    fn encode(&self, mut encoder: Encoder) -> Result<(), std::io::Error> {
        encoder
            .with_label_set(&self.0)
            .encode_suffix("info")?
            .no_bucket()?
            .encode_value(1u32)?
            .no_exemplar()?;

        Ok(())
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::counter::Counter;
    use crate::metrics::gauge::Gauge;
    use crate::metrics::histogram::exponential_buckets;
    use pyo3::{prelude::*, types::PyModule};
    use std::borrow::Cow;

    #[test]
    fn encode_counter() {
        let counter: Counter = Counter::default();
        let mut registry = Registry::default();
        registry.register("my_counter", "My counter", counter.clone());

        let mut encoded = Vec::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_counter_with_unit() {
        let mut registry = Registry::default();
        let counter: Counter = Counter::default();
        registry.register_with_unit("my_counter", "My counter", Unit::Seconds, counter.clone());

        let mut encoded = Vec::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_counter_seconds My counter.\n".to_owned()
            + "# TYPE my_counter_seconds counter\n"
            + "# UNIT my_counter_seconds seconds\n"
            + "my_counter_seconds_total 0\n"
            + "# EOF\n";
        assert_eq!(expected, String::from_utf8(encoded.clone()).unwrap());

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_counter_with_exemplar() {
        let mut registry = Registry::default();

        let counter_with_exemplar: CounterWithExemplar<(String, u64)> =
            CounterWithExemplar::default();
        registry.register_with_unit(
            "my_counter_with_exemplar",
            "My counter with exemplar",
            Unit::Seconds,
            counter_with_exemplar.clone(),
        );

        counter_with_exemplar.inc_by(1, Some(("user_id".to_string(), 42)));

        let mut encoded = Vec::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_counter_with_exemplar_seconds My counter with exemplar.\n"
            .to_owned()
            + "# TYPE my_counter_with_exemplar_seconds counter\n"
            + "# UNIT my_counter_with_exemplar_seconds seconds\n"
            + "my_counter_with_exemplar_seconds_total 1 # {user_id=\"42\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, String::from_utf8(encoded.clone()).unwrap());

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_gauge() {
        let mut registry = Registry::default();
        let gauge: Gauge = Gauge::default();
        registry.register("my_gauge", "My gauge", gauge.clone());

        let mut encoded = Vec::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
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

        let mut encoded = Vec::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
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

        let mut encoded = Vec::new();

        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_prefix_my_counter_family My counter family.\n"
            .to_owned()
            + "# TYPE my_prefix_my_counter_family counter\n"
            + "my_prefix_my_counter_family_total{my_key=\"my_value\",method=\"GET\",status=\"200\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, String::from_utf8(encoded.clone()).unwrap());

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_info() {
        let mut registry = Registry::default();
        let info = Info::new(vec![("os".to_string(), "GNU/linux".to_string())]);
        registry.register("my_info_metric", "My info metric", info);

        let mut encoded = Vec::new();
        encode(&mut encoded, &registry).unwrap();

        let expected = "# HELP my_info_metric My info metric.\n".to_owned()
            + "# TYPE my_info_metric info\n"
            + "my_info_metric_info{os=\"GNU/linux\"} 1\n"
            + "# EOF\n";
        assert_eq!(expected, String::from_utf8(encoded.clone()).unwrap());

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_histogram() {
        let mut registry = Registry::default();
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);

        let mut encoded = Vec::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
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

        let mut encoded = Vec::new();

        encode(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_histogram_with_exemplars() {
        let mut registry = Registry::default();
        let histogram = HistogramWithExemplars::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0, Some(("user_id".to_string(), 42u64)));

        let mut encoded = Vec::new();
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
        assert_eq!(expected, String::from_utf8(encoded.clone()).unwrap());

        parse_with_python_client(String::from_utf8(encoded).unwrap());
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
                .call1((input,))
                .map_err(|e| e.to_string())
                .unwrap();
        })
    }
}
