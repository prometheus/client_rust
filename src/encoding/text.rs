use crate::counter::{Atomic, Counter};
use crate::label::LabelSet;
use crate::registry::Registry;
use std::io::Write;
use std::iter::{once, Once};

fn encode<W: Write, M: IterSamples<S>, S>(
    writer: &mut W,
    registry: &Registry<M>,
) -> Result<(), std::io::Error> {
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

        for sample in metric.iter_samples() {
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

struct Sample<S> {
    suffix: Option<String>,
    labels: Option<S>,
    value: String,
}

trait IterSamples<S>
where
    Self::IntoIter: Iterator<Item = Sample<S>>,
{
    type IntoIter: Iterator;

    fn iter_samples(&self) -> Self::IntoIter;
}

impl<A, S> IterSamples<S> for Counter<A>
where
    A: Atomic,
    A::Number: ToString,
{
    type IntoIter = Once<Sample<S>>;

    fn iter_samples(&self) -> Once<Sample<S>> {
        once(Sample {
            suffix: None,
            labels: None,
            value: self.get().to_string(),
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
        registry.register(Descriptor::new("counter", "My counter", "my_counter"), counter.clone());

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
