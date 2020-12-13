use crate::counter::{Atomic, Counter};
use crate::family::MetricFamily;
use crate::label::LabelSet;
use crate::registry::Registry;
use std::io::Write;
use std::iter::{once, Once};

fn encode<W, M, S>(writer: &mut W, registry: &Registry<M>) -> Result<(), std::io::Error>
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
            // TODO: Only do if counter.
            writer.write(b"_total")?;

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

trait Encode {
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

struct Sample<S> {
    suffix: Option<String>,
    labels: Option<S>,
    value: String,
}

trait ForEachSample<S> {
    fn for_each<E, F: FnMut(Sample<S>) -> Result<(), E>>(&self, f: F) -> Result<(), E>;
}

impl<A, S> ForEachSample<S> for Counter<A>
where
    A: Atomic,
    <A as Atomic>::Number: ToString,
{
    fn for_each<E, F: FnMut(Sample<S>) -> Result<(), E>>(&self, mut f: F) -> Result<(), E> {
        f(Sample {
            suffix: None,
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
