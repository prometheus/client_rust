# Open Metrics Client Library

[Rust](https://github.com/rust-lang/) client library implementation of the [Open
Metrics specification](https://github.com/OpenObservability/OpenMetrics). Allows
developers to instrument applications and thus enables operators to monitor said
applications with monitoring systems like [Prometheus](https://prometheus.io/).

**Documentation**: https://docs.rs/open-metrics-client/

## Goals

- No `unsafe`. Don't use unsafe Rust within the library itself.

- Type safe. Leverage Rust's type system to catch common instrumentation
  mistakes at compile time.

- Fast. Don't force users to worry about the performance impact of
  instrumentation. Instead encourage users to instrument often and extensively.

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

#### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
