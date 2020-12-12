use crate::counter::{Atomic, Counter};
use crate::family::MetricFamily;
use crate::label::LabelSet;
use crate::registry::Registry;
use std::io::Write;
use std::iter::{once, Once};

fn encode<'a, W, M, S: 'a>(writer: &mut W, registry: &Registry<M>) -> Result<(), std::io::Error>
where
    W: Write,
    M: IterSamples<'a, S>,
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

        for sample in (&metric).iter_samples() {
            writer.write(desc.name().as_bytes())?;
            // TODO: Only do if counter.
            writer.write(b"_total")?;
            writer.write(" ".as_bytes())?;
            writer.write(sample.value.as_bytes())?;
            writer.write(b"\n")?;
        }
    }

    writer.write("# EOF\n".as_bytes())?;

    Ok(())
}

struct Sample<'a, S> {
    suffix: Option<String>,
    labels: Option<&'a S>,
    value: String,
}

trait IterSamples<'a, S: 'a>
where
    Self::IntoIter: 'a + Iterator<Item = Sample<'a, S>>,
{
    type IntoIter;

    fn iter_samples(&self) -> Self::IntoIter;
}

impl<'a, A, S: 'a> IterSamples<'a, S> for Counter<A>
where
    A: Atomic,
    A::Number: ToString,
{
    type IntoIter = Once<Sample<'a, S>>;

    fn iter_samples(&self) -> Self::IntoIter {
        once(Sample {
            suffix: None,
            labels: None,
            value: self.get().to_string(),
        })
    }
}

impl<'a, S, M> IterSamples<'a, S> for &'a MetricFamily<S, M>
where
    S: 'a + Eq + std::hash::Hash + LabelSet + Clone,
    M: 'a + IterSamples<'a, S>,
{
    type IntoIter = MetricFamilySampleIter<'a, S, M>;

    fn iter_samples(&self) -> MetricFamilySampleIter<'a, S, M> {
        MetricFamilySampleIter { iter: self.iter() }
    }
}

struct MetricFamilySampleIter<'a, S, M> {
    iter: std::collections::hash_map::Iter<'a, S, M>,
}

impl<'a, S, M> Iterator for MetricFamilySampleIter<'a, S, M>
where
    S: Clone,
    M: IterSamples<'a, S>,
{
    type Item = Sample<'a, S>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|(label_set, metric)| {
            // TODO: Remove `unwrap`. Today only expecting a single sample, thus
            // not allowing nested metric families. Think about whether nesting
            // is a good idea in the first place.
            let mut sample = metric.iter_samples().next().unwrap();
            sample.labels = Some(label_set);
            sample
        })
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
    fn register_and_iterate() {
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
