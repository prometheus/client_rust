mod open_metrics_proto {
    include!(concat!(env!("OUT_DIR"), "/openmetrics.rs"));
}

fn foo() {
    let label = open_metrics_proto::Label {
        name: "foo".to_string(),
        value: "bar".to_string(),
    };
}
