use prometheus_client::metrics::gauge::Gauge;
use prometheus_client_derive::Registrant;

#[derive(Registrant)]
struct Server {
    #[registrant(unit = "bytes")]
    mem_usage: Gauge,

    #[registrant(unit = bytes)]
    invalid: Gauge,
}

fn main() {}
