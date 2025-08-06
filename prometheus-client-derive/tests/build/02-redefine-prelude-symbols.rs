#![allow(unused_imports)]

// Empty module has nothing and can be used to redefine symbols.
mod empty {}

// redefine the prelude `::std`
use empty as std;

// redefine the dependency `::prometheus_client`
use empty as prometheus_client;

// redefine the dependency `::prometheus_client_derive`
use empty as prometheus_client_derive;

// redefine the prelude `::core::result::Result`.
type Result = ();

enum TResult {
    Ok,
    Err,
}

// redefine the prelude `::core::result::Result::Ok/Err`.
use TResult::Ok;
use TResult::Err;

type Option = ();

enum TOption {
    Some,
    None,
}

// redefine the prelude `::core::option::Option::Some/None`.
use TOption::Some;
use TOption::None;

#[derive(::prometheus_client_derive::Registrant)]
struct Server {
    #[registrant(rename = "requests")]
    /// Number of HTTP requests received
    /// from the client
    reqs: ::prometheus_client::metrics::counter::Counter,

    #[registrant(unit = "bytes")]
    /// Memory usage in bytes
    /// of the server
    mem_usage: ::prometheus_client::metrics::gauge::Gauge,

    #[registrant(skip)]
    _phantom: (),
}

fn main() {}
