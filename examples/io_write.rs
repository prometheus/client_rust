//! Example showing how one could write to a file or socket instead of a string.
//! For large metrics registries this will be more memory efficient.

use prometheus_client::{encoding::text::encode, metrics::counter::Counter, registry::Registry};
use std::io::Write;

fn main() {
    let mut registry = <Registry>::with_prefix("stream");
    let request_counter: Counter<u64> = Default::default();

    registry.register(
        "requests",
        "How many requests the application has received",
        request_counter.clone(),
    );

    let mut buf = String::new();
    encode(&mut buf, &registry).unwrap();

    let mut file = Vec::new();
    let mut writer = IoWriterWrapper(&mut file);
    encode(&mut writer, &registry).unwrap();

    assert!(buf.as_bytes() == file);
}

pub struct IoWriterWrapper<W>(W);

impl<W> std::fmt::Write for IoWriterWrapper<W>
where
    W: Write,
{
    fn write_str(&mut self, input: &str) -> std::fmt::Result {
        self.0
            .write_all(input.as_bytes())
            .map(|_| ())
            .map_err(|_| std::fmt::Error)
    }
}
