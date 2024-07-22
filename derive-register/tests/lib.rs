use prometheus_client::{
    encoding::text::encode,
    metrics::{counter::Counter, gauge::Gauge},
    registry::{Register, RegisterDefault, Registry},
};

#[derive(Register, Default)]
struct Metrics {
    /// This is my counter
    my_counter: Counter,
    nested: NestedMetrics,
    #[register(skip)]
    skipped: Counter,
    #[register(unit = "bytes")]
    custom_unit: Counter,
    #[register(name = "my_custom_name")]
    custom_name: Counter,
}

#[derive(Register, Default)]
struct NestedMetrics {
    /// This is my gauge
    my_gauge: Gauge,
}

#[test]
fn basic_flow() {
    let mut registry = Registry::default();

    let metrics = Metrics::register_default(&mut registry);

    metrics.my_counter.inc();
    metrics.nested.my_gauge.set(23);

    // Encode all metrics in the registry in the text format.
    let mut buffer = String::new();
    encode(&mut buffer, &registry).unwrap();

    let expected = "# HELP my_counter This is my counter.\n".to_owned()
        + "# TYPE my_counter counter\n"
        + "my_counter_total 1\n"
        + "# HELP custom_unit_bytes .\n"
        + "# TYPE custom_unit_bytes counter\n"
        + "# UNIT custom_unit_bytes bytes\n"
        + "custom_unit_bytes_total 0\n"
        + "# HELP my_custom_name .\n"
        + "# TYPE my_custom_name counter\n"
        + "my_custom_name_total 0\n"
        + "# HELP nested_my_gauge This is my gauge.\n"
        + "# TYPE nested_my_gauge gauge\n"
        + "nested_my_gauge 23\n"
        + "# EOF\n";
    assert_eq!(expected, buffer);
}
