use prometheus_client::metrics::counter::Counter;
use prometheus_client_derive::Registrant;

#[derive(Registrant)]
struct Server {
    #[registrant(rename = "http_requests")]
    requests: Counter,

    #[registrant(rename = http_requests)]
    invalid: Counter,
}

fn main() {}
