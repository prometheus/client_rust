# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.21.2]

### Added

- Added `sub_registry_with_labels` method to `Registry`.
  See [PR 145].
- Added `with_labels` and `with_prefix_and_labels` constructors to `Registry`.
  See [PR 147].

[PR 145]: https://github.com/prometheus/client_rust/pull/145
[PR 147]: https://github.com/prometheus/client_rust/pull/147

## [0.21.1]

### Added

- Implement `EncodeLabelValue` for `Option<T>`.
  See [PR 137].

[PR 137]: https://github.com/prometheus/client_rust/pull/137

## [0.21.0]

### Changed

- Replace `impl EncodeMetric for RefCell<T>` with a new type `ConstFamily` implementing `EncodeMetric`.

## [0.20.0]

### Added

- Introduce `Collector` abstraction allowing users to provide additional metrics
  and their description on each scrape. See [PR 82].

- Introduce a `#[prometheus(flatten)]` attribute which can be used when deriving `EncodeLabelSet`, allowing
  a nested struct to be flattened during encoding. See [PR 118].

  For example:

  ```rust
  #[derive(EncodeLabelSet, Hash, Clone, Eq, PartialEq, Debug)]
  struct CommonLabels {
      a: u64,
      b: u64,
  }
  #[derive(EncodeLabelSet, Hash, Clone, Eq, PartialEq, Debug)]
  struct Labels {
      unique: u64,
      #[prometheus(flatten)]
      common: CommonLabels,
  }
  ```

  Would be encoded as:

  ```
  my_metric{a="42",b="42",unique="42"} 42
  ```

### Fixed

- Fix label encoding in protobuf feature. See [PR 123].

[PR 82]: https://github.com/prometheus/client_rust/pull/82
[PR 118]: https://github.com/prometheus/client_rust/pull/118
[PR 123]: https://github.com/prometheus/client_rust/pull/123

## [0.19.0]

This is a large release including multiple breaking changes. Major user-facing
improvement of this release is support for the OpenMetrics Protobuf format.

### Upgrade guide:

- Don't box before registering.

  ```diff
    registry.register(
        "my_metric",
        "This is my metric",
  -      Box::new(my_metric.clone()),
  +      my_metric.clone(),
    );
  ```

- Gauge uses `i64` instead of `u64`.

  ```diff
    my_gauge
  -     .set(42u64);
  +     .set(42i64);
  ```

- Derive `EncodeLabelSet` for `struct` and `EncodeLabelValue` for `enum` instead of just `Encode` for all and require `Debug`.

  ```diff
  - #[derive(Clone, Hash, PartialEq, Eq, Encode)]
  + #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelSet, Debug)]
    struct Labels {
        path: String,
        method: Method,
        some_number: u64,
    }

  - #[derive(Clone, Hash, PartialEq, Eq, Encode)]
  + #[derive(Clone, Hash, PartialEq, Eq, EncodeLabelValue, Debug)]
    enum Method {
        Get,
        #[allow(dead_code)]
        Put,
    }
  ```

- Encode as utf-8 and not as `[u8]`.

  ```diff
  - let mut buffer = vec![];
  + let mut buffer = String::new();
    encode(&mut buffer, &registry).unwrap();
  ```

For details on each of these, see changelog entries below.

### Added

- Added support for the OpenMetrics protobuf format. See [PR 83].
- Added a `remove` method to `Family` to allow the removal of a specified label
  set from a family. See [PR 85].
- Added a `clear` method to `Family` to allow the removal of all label sets
  from a family. See [PR 85].
- Impl `TypedMetric` for `CounterWithExemplar` and `HistogramWithExemplar`, so that they can be used with `Family`. See [PR 96].

### Changed

- Always use dynamic dispatch on `Registry`, i.e. remove generic type parameter `M` from `Registry`. See [PR 105].
- Refactor encoding. See [PR 105].
  - Introducing separate traits to encode
    - value (e.g. `EncodeCounterValue`)
    - label set (`EncodeLabelSet`), derivable for structs via `prometheus-client-derive-encode`
    - label (`EncodeLabel`)
    - label key (`EncodeLabelKey`)
    - label value (`EncodeLabelValue`), derivable for enums via `prometheus-client-derive-encode`
  - Encode as UTF-8 strings, not bytes. I.e. use `std::fmt::Write` instead of `std::io::Write`.
- Use signed integers for `Gauge` for compliance with OpenMetrics protobuf
  format. See [PR 105].

[PR 83]: https://github.com/prometheus/client_rust/pull/83
[PR 85]: https://github.com/prometheus/client_rust/pull/85
[PR 96]: https://github.com/prometheus/client_rust/pull/96
[PR 105]: https://github.com/prometheus/client_rust/pull/105

## [0.18.1]

### Fixed

- Fix race condition in `Family::get_or_create`. See [PR 102].

[PR 102]: https://github.com/prometheus/client_rust/pull/102

## [0.18.0]

### Changed

- Use `parking_lot` instead of `std::sync::*`.

  Before `proemtheus-client` would use the `owning_ref` crate to map the target
  of a `std::sync::RwLockReadGuard`. `owning_ref` has multiple unsoundness
  issues, see https://rustsec.org/advisories/RUSTSEC-2022-0040.html. Instead of
  replacing `owning_ref` with a similar crate, we switch to locking via
  `parking_lot` which supports the above mapping natively.

  See [PR 78] and [issue 77].

[PR 78]: https://github.com/prometheus/client_rust/pull/78
[issue 77]: https://github.com/prometheus/client_rust/issues/77

## [0.17.0]

### Changed
- Updates to Rust 2021 Edition. See [PR 65].

### Added
- Added a `with_prefix` method to `Registry` to allow initializing a registry with a prefix. See [PR 70].
- Added `Debug` implementations on most public types that were missing them. See [PR 71].
- Added example for actix-web framework. See [PR 76].

### Removed
- Remove `Add` trait implementation for a private type which lead to compile time conflicts with existing `Add` implementations e.g. on `String`. See [PR 69].

[PR 65]: https://github.com/prometheus/client_rust/pull/65
[PR 69]: https://github.com/prometheus/client_rust/pull/69
[PR 70]: https://github.com/prometheus/client_rust/pull/70
[PR 71]: https://github.com/prometheus/client_rust/pull/71
[PR 76]: https://github.com/prometheus/client_rust/pull/76

## [0.16.0]

### Changed

- Require `Registry` default generic type `SendEncodeMetric` to be `Sync`. See [PR 58].

[PR 58]: https://github.com/prometheus/client_rust/pull/58

## [0.15.1] - 2022-02-04

### Added

- Expose `Encoder` methods. See [PR 41].

### Changed

- Use `AtomicU32` on platforms that don't support `AtomicU64`. See [PR 42].

[PR 41]: https://github.com/prometheus/client_rust/pull/41
[PR 42]: https://github.com/prometheus/client_rust/pull/42

## [0.15.0] - 2022-01-16

### Changed

- Release as `prometheus-client` and `prometheus-client-derive-text-encode`.

## [0.14.0] - 2021-12-29

### Changed

- Update to `itoa` `v1`. See [PR 28].
- Update to `dtoa` `v1`. See [PR 27].

### Added

- Implement `Gauge::dec` and `Gauge::dec_by`. See [PR 30].

[PR 28]: https://github.com/prometheus/client_rust/pull/28
[PR 27]: https://github.com/prometheus/client_rust/pull/27
[PR 30]: https://github.com/prometheus/client_rust/pull/30

## [0.13.0] - 2021-11-21

_Note: This was initially released as `v0.12.1` but later on yanked due to it
including a breaking change. See [PR 24] for details._

### Added

- Allow family to use constructors that do not coerce to function pointers. See [PR 21].

[PR 21]: https://github.com/prometheus/client_rust/pull/21
[PR 24]: https://github.com/prometheus/client_rust/pull/24

## [0.12.0] - 2021-08-07

### Added

- Add `Registry::sub_registry_with_label`. See [PR 20].

### Changed

- Rename `Registry::sub_registry` to `Registry::sub_registry_with_prefix`. See
  [PR 20].

[PR 20]: https://github.com/prometheus/client_rust/pull/20

## [0.11.2] - 2021-06-09
### Fixed
- Do not separate labels with spaces.

## [0.11.1] - 2021-06-08
### Fixed
- Encode Info metric labels.

## [0.11.0] - 2021-06-08
### Added
- Add support for OpenMetrics Info metrics (see [PR 18]).

[PR 18]: https://github.com/prometheus/client_rust/pull/18

## [0.10.1] - 2021-05-31
### Added
- Implement `Encode` for `u32`.

### Fixed
- Update to prometheus-client-derive-text-encode v0.1.1 which handles keyword
  identifiers aka raw identifiers

  https://github.com/prometheus/client_rust/pull/16

## [0.10.0] - 2021-04-29
### Added
- Added `metrics::histogram::linear_buckets`.
  https://github.com/prometheus/client_rust/issues/13

### Changed
- Renamed `metrics::histogram::exponential_series` to
  `metrics::histogram::exponential_buckets`.
  https://github.com/prometheus/client_rust/issues/13
