use crate::counter::{Atomic, Counter};
use crate::family::MetricFamily;
use crate::histogram::Histogram;
use crate::label::LabelSet;
use crate::registry::Registry;
use std::borrow::Cow;
use std::io::Write;

pub fn encode<W, M, S>(writer: &mut W, registry: &Registry<M>) -> Result<(), std::io::Error>
where
    W: Write,
    M: ForEachSample<S>,
    S: Encode,
{
    for (desc, metric) in registry.iter() {
        writer.write(b"# HELP ")?;
        writer.write(desc.name().as_bytes())?;
        writer.write(b" ")?;
        writer.write(desc.help().as_bytes())?;
        writer.write(b"\n")?;

        writer.write(b"# TYPE ")?;
        writer.write(desc.name().as_bytes())?;
        writer.write(b" ")?;
        writer.write(desc.m_type().as_bytes())?;
        writer.write(b"\n")?;

        metric.for_each(|sample| -> Result<(), std::io::Error> {
            writer.write(desc.name().as_bytes())?;

            if let Some(suffix) = sample.suffix {
                writer.write("_".as_bytes())?;
                writer.write(suffix.as_bytes())?;
            }

            if let Some(label_set) = sample.labels {
                label_set.encode(writer)?;
            }

            writer.write(" ".as_bytes())?;
            writer.write(sample.value.as_bytes())?;
            writer.write(b"\n")?;
            Ok(())
        })?;
    }

    writer.write("# EOF\n".as_bytes())?;

    Ok(())
}

pub trait Encode {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), std::io::Error>;
}

impl Encode for Vec<(String, String)> {
    fn encode<W: Write>(&self, writer: &mut W) -> Result<(), std::io::Error> {
        if self.is_empty() {
            return Ok(());
        }

        writer.write(b"{")?;

        let mut iter = self.iter().peekable();
        while let Some((name, value)) = iter.next() {
            writer.write(name.as_bytes())?;
            writer.write(b"=\"")?;
            writer.write(value.as_bytes())?;
            writer.write(b"\"")?;

            if iter.peek().is_some() {
                writer.write(b",")?;
            }
        }

        writer.write(b"}")?;

        Ok(())
    }
}

pub struct Sample<S> {
    suffix: Option<Cow<'static, str>>,
    labels: Option<S>,
    // TODO: Don't use String here. Likely an unneeded allocation. For integers
    // itoa might bring some performance.
    value: String,
}

pub trait ForEachSample<S> {
    fn for_each<E, F: FnMut(Sample<S>) -> Result<(), E>>(&self, f: F) -> Result<(), E>;
}

impl<A, S> ForEachSample<S> for Counter<A>
where
    A: Atomic,
    <A as Atomic>::Number: ToString,
{
    fn for_each<E, F: FnMut(Sample<S>) -> Result<(), E>>(&self, mut f: F) -> Result<(), E> {
        f(Sample {
            suffix: Some("total".into()),
            labels: None,
            value: self.get().to_string(),
        })
    }
}

impl<S, M> ForEachSample<S> for MetricFamily<S, M>
where
    S: Clone + LabelSet + std::hash::Hash + Eq,
    M: Default + ForEachSample<S>,
{
    fn for_each<E, F: FnMut(Sample<S>) -> Result<(), E>>(&self, mut f: F) -> Result<(), E> {
        let guard = self.read();
        let mut iter = guard.iter();
        while let Some((label_set, m)) = iter.next() {
            m.for_each(|s: Sample<S>| {
                let mut s = s;
                // TODO: Would be ideal to get around this clone.
                s.labels = Some(label_set.clone());
                f(s)
            })?;
        }
        Ok(())
    }
}

// TODO: Make sure labels are not overwritten when used with `MetricFamily`.
impl ForEachSample<Vec<(String, String)>> for Histogram {
    fn for_each<E, F: FnMut(Sample<Vec<(String, String)>>) -> Result<(), E>>(
        &self,
        mut f: F,
    ) -> Result<(), E> {
        f(Sample {
            suffix: Some("sum".into()),
            labels: None,
            value: self.sum().to_string(),
        })?;

        f(Sample {
            suffix: Some("count".into()),
            labels: None,
            value: self.count().to_string(),
        })?;

        for (upper_bound, count) in self.buckets().iter() {
            let label = (
                "le".to_string(),
                if *upper_bound == f64::MAX {
                    "+Inf".to_string()
                } else {
                    upper_bound.to_string()
                },
            );
            f(Sample {
                suffix: Some("bucket".into()),
                labels: Some(vec![label]),
                value: count.to_string(),
            })?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::counter::Counter;
    use crate::registry::Descriptor;
    use pyo3::{prelude::*, types::PyModule};
    use std::sync::atomic::AtomicU64;

    #[test]
    fn encode_counter() {
        let mut registry = Registry::new();
        let counter = Counter::<AtomicU64>::new();
        registry.register(
            Descriptor::new("counter", "My counter", "my_counter"),
            counter.clone(),
        );

        let mut encoded = Vec::new();

        encode::<_, _, Vec<(String, String)>>(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_counter_family() {
        let mut registry = Registry::new();
        let family = MetricFamily::<Vec<(String, String)>, Counter<AtomicU64>>::new();
        registry.register(
            Descriptor::new("counter", "My counter family", "my_counter_family"),
            family.clone(),
        );

        family
            .get_or_create(&vec![("method".to_string(), "GET".to_string())])
            .inc();

        let mut encoded = Vec::new();

        encode::<_, _, Vec<(String, String)>>(&mut encoded, &registry).unwrap();

        parse_with_python_client(String::from_utf8(encoded).unwrap());
    }

    #[test]
    fn encode_histogram() {
        let mut registry = Registry::new();
        let histogram = Histogram::new(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0]);
        registry.register(
            Descriptor::new("histogram", "My histogram", "my_histogram"),
            histogram.clone(),
        );
        histogram.observe(1.0);

        let mut encoded = Vec::new();

        encode::<_, _, Vec<(String, String)>>(&mut encoded, &registry).unwrap();

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
