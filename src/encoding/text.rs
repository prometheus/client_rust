use crate::counter::{self, Counter};
use crate::family::Family;
use crate::gauge::{self, Gauge};
use crate::histogram::Histogram;
use crate::registry::Registry;

use std::io::Write;
use std::ops::Deref;

pub fn encode<W, M>(writer: &mut W, registry: &Registry<M>) -> Result<(), std::io::Error>
where
    W: Write,
    M: EncodeMetric,
{
    for (desc, metric) in registry.iter() {
        writer.write_all(b"# HELP ")?;
        writer.write_all(desc.name().as_bytes())?;
        writer.write_all(b" ")?;
        writer.write_all(desc.help().as_bytes())?;
        writer.write_all(b"\n")?;

        writer.write_all(b"# TYPE ")?;
        writer.write_all(desc.name().as_bytes())?;
        writer.write_all(b" ")?;
        writer.write_all(desc.m_type().as_bytes())?;
        writer.write_all(b"\n")?;

        let encoder = Encoder {
            writer,
            name: &desc.name(),
            labels: None,
        };

        metric.encode(encoder)?;
    }

    writer.write_all(b"# EOF\n")?;

    Ok(())
}

// `Encoder` does not take a trait parameter for `writer` and `labels` because
// `EncodeMetric` which uses `Encoder` needs to be usable as a trait object in
// order to be able to register different metric types with a `Registry`. Trait
// objects can not use type parameters.
//
// TODO: Alternative solutions to the above are very much appreciated.
pub struct Encoder<'a, 'b> {
    writer: &'a mut dyn Write,
    name: &'a str,
    labels: Option<&'b dyn Encode>,
}

impl<'a, 'b> Encoder<'a, 'b> {
    pub fn encode_suffix(&mut self, suffix: &'static str) -> Result<BucketEncoder, std::io::Error> {
        self.writer.write_all(self.name.as_bytes())?;
        self.writer.write_all(b"_")?;
        self.writer.write_all(suffix.as_bytes()).map(|_| ())?;

        self.encode_labels()
    }

    pub fn no_suffix(&mut self) -> Result<BucketEncoder, std::io::Error> {
        self.writer.write_all(self.name.as_bytes())?;

        self.encode_labels()
    }

    pub(self) fn encode_labels(&mut self) -> Result<BucketEncoder, std::io::Error> {
        if let Some(labels) = &self.labels {
            self.writer.write_all(b"{")?;
            labels.encode(self.writer)?;

            Ok(BucketEncoder {
                opened_curly_brackets: true,
                writer: self.writer,
            })
        } else {
            Ok(BucketEncoder {
                opened_curly_brackets: false,
                writer: self.writer,
            })
        }
    }

    pub fn with_label_set<'c, 'd>(&'c mut self, label_set: &'d dyn Encode) -> Encoder<'c, 'd> {
        debug_assert!(self.labels.is_none());

        Encoder {
            writer: self.writer,
            name: self.name,
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
    fn encode_bucket<K: Encode, V: Encode>(
        &mut self,
        key: K,
        value: V,
    ) -> Result<ValueEncoder, std::io::Error> {
        if self.opened_curly_brackets {
            self.writer.write_all(b", ")?;
        } else {
            self.writer.write_all(b"{")?;
        }

        key.encode(self.writer)?;
        self.writer.write_all(b"=\"")?;
        value.encode(self.writer)?;
        self.writer.write_all(b"\"}")?;

        Ok(ValueEncoder {
            writer: self.writer,
        })
    }

    fn no_bucket(&mut self) -> Result<ValueEncoder, std::io::Error> {
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
    fn encode_value<V: Encode>(&mut self, v: V) -> Result<(), std::io::Error> {
        self.writer.write_all(b" ")?;
        v.encode(self.writer)?;
        self.writer.write_all(b"\n")?;
        Ok(())
    }
}

pub trait EncodeMetric {
    fn encode(&self, encoder: Encoder) -> Result<(), std::io::Error>;
}

impl EncodeMetric for Box<dyn EncodeMetric> {
    fn encode(&self, encoder: Encoder) -> Result<(), std::io::Error> {
        self.deref().encode(encoder)
    }
}

pub trait Encode {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error>;
}

impl Encode for () {
    fn encode(&self, _writer: &mut dyn Write) -> Result<(), std::io::Error> {
        Ok(())
    }
}

impl Encode for f64 {
    fn encode(&self, mut writer: &mut dyn Write) -> Result<(), std::io::Error> {
        dtoa::write(&mut writer, *self)?;
        Ok(())
    }
}

impl Encode for u64 {
    fn encode(&self, mut writer: &mut dyn Write) -> Result<(), std::io::Error> {
        itoa::write(&mut writer, *self)?;
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

impl Encode for Vec<(String, String)> {
    fn encode(&self, writer: &mut dyn Write) -> Result<(), std::io::Error> {
        if self.is_empty() {
            return Ok(());
        }

        let mut iter = self.iter().peekable();
        while let Some((name, value)) = iter.next() {
            writer.write_all(name.as_bytes())?;
            writer.write_all(b"=\"")?;
            writer.write_all(value.as_bytes())?;
            writer.write_all(b"\"")?;

            if iter.peek().is_some() {
                writer.write_all(b",")?;
            }
        }

        Ok(())
    }
}

impl<A> EncodeMetric for Counter<A>
where
    A: counter::Atomic,
    <A as counter::Atomic>::Number: Encode,
{
    fn encode(&self, mut encoder: Encoder) -> Result<(), std::io::Error> {
        encoder
            .encode_suffix("total")?
            .no_bucket()?
            .encode_value(self.get())?;

        Ok(())
    }
}

impl<A> EncodeMetric for Gauge<A>
where
    A: gauge::Atomic,
    <A as gauge::Atomic>::Number: Encode,
{
    fn encode(&self, mut encoder: Encoder) -> Result<(), std::io::Error> {
        encoder.no_suffix()?.no_bucket()?.encode_value(self.get())?;

        Ok(())
    }
}

impl<S, M> EncodeMetric for Family<S, M>
where
    S: Clone + std::hash::Hash + Eq + Encode,
    M: EncodeMetric,
{
    fn encode(&self, mut encoder: Encoder) -> Result<(), std::io::Error> {
        let guard = self.read();
        for (label_set, m) in guard.iter() {
            let encoder = encoder.with_label_set(label_set);
            m.encode(encoder)?;
        }
        Ok(())
    }
}

impl EncodeMetric for Histogram {
    fn encode(&self, mut encoder: Encoder) -> Result<(), std::io::Error> {
        let (sum, count, buckets) = self.get();
        encoder
            .encode_suffix("sum")?
            .no_bucket()?
            .encode_value(sum)?;
        encoder
            .encode_suffix("count")?
            .no_bucket()?
            .encode_value(count)?;

        for (upper_bound, count) in buckets.iter() {
            let bucket_key = if *upper_bound == f64::MAX {
                "+Inf".to_string()
            } else {
                upper_bound.to_string()
            };

            encoder
                .encode_suffix("bucket")?
                .encode_bucket("le", bucket_key.as_str())?
                .encode_value(*count)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counter::Counter;
    use crate::gauge::Gauge;
    use crate::histogram::exponential_series;
    use crate::registry::Descriptor;
    use pyo3::{prelude::*, types::PyModule};
    use std::sync::atomic::AtomicU64;

    #[test]
    fn encode_counter() {
        let mut registry = Registry::default();
        let counter = Counter::<AtomicU64>::new();
        registry.register(
            Descriptor::new("counter", "My counter", "my_counter"),
            counter.clone(),
        );

        let mut encoded = Vec::new();

        encode::<_, _>(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_gauge() {
        let mut registry = Registry::default();
        let gauge = Gauge::<AtomicU64>::new();
        registry.register(
            Descriptor::new("gauge", "My gauge", "my_gauge"),
            gauge.clone(),
        );

        let mut encoded = Vec::new();

        encode::<_, _>(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_counter_family() {
        let mut registry = Registry::default();
        let family = Family::<Vec<(String, String)>, Counter<AtomicU64>>::default();
        registry.register(
            Descriptor::new("counter", "My counter family", "my_counter_family"),
            family.clone(),
        );

        family
            .get_or_create(&vec![("method".to_string(), "GET".to_string())])
            .inc();

        let mut encoded = Vec::new();

        encode::<_, _>(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_histogram() {
        let mut registry = Registry::default();
        let histogram = Histogram::new(exponential_series(1.0, 2.0, 10));
        registry.register(
            Descriptor::new("histogram", "My histogram", "my_histogram"),
            histogram.clone(),
        );
        histogram.observe(1.0);

        let mut encoded = Vec::new();

        encode::<_, _>(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    fn parse_with_python_client(input: String) {
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
                .call1("parse", (input,))
                .map_err(|e| e.to_string())
                .unwrap();
        })
    }
}
