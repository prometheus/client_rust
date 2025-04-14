#![allow(unused_imports)]

// empty module has nothing and be used to redefine symbols
mod empty {}

// redefine the prelude `::std`
use empty as std;

// redefine the dependency `::prometheus_client`
use empty as prometheus_client;

// redefine the prelude `::core::result::Result`.
type Result = ();

enum TResult {
    Ok,
    Err,
}

// redefine the prelude `::core::result::Result::Ok/Err`.
use TResult::Ok;
use TResult::Err;

#[derive(Debug, Clone, PartialEq, Eq, Hash, ::prometheus_client::encoding::EncodeLabelSet)]
struct LableSet {
    a: String,
    b: LabelEnum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ::prometheus_client::encoding::EncodeLabelValue)]
enum LabelEnum {
    A,
    B,
}

fn main() {}
