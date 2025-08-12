use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client_derive::Registrant;

#[derive(Registrant)]
struct Server {
    /// One line help
    requests: Counter,

    /// Muti-line help
    /// with a lot of text
    mem_usage: Gauge,
}

fn main() {}
