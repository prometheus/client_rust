#![allow(unused_imports)]
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client_derive::Registrant;

#[derive(Registrant)]
struct Server {
    #[registrant(rename = "memory_usage", unit = "bytes")] // mutiple attributes in single parenthesis
    mem_usage: Gauge,

    #[registrant(rename = "tcp_retransmitted")]
    #[registrant(unit = "segments")]
    tcp_retrans: Gauge,
}

fn main() {}
