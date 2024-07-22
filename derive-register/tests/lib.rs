use prometheus_client::{
    encoding::text::encode,
    metrics::counter::Counter,
    registry::{Register, Registry},
};

#[derive(Register, Default, Clone)]
struct Metrics {
    /// This is my counter
    my_counter: Counter,
}

#[test]
fn basic_flow() {
    let mut registry = Registry::default();

    let metrics = Metrics::default();
    metrics.clone().register(&mut registry);

    metrics.my_counter.inc();

    // Encode all metrics in the registry in the text format.
    let mut buffer = String::new();
    encode(&mut buffer, &registry).unwrap();

    let expected = "# HELP my_counter This is my counter.\n".to_owned()
        + "# TYPE my_counter counter\n"
        + "my_counter_total 1\n"
        + "# EOF\n";
    assert_eq!(expected, buffer);
}
