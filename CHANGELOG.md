# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.19.0] - unreleased

### Added
- Added support for the OpenMetrics protobuf format. See [PR 83].
- Added a `remove` method to `Family` to allow the removal of a specified label
  set from a family. See [PR 85].
- Added a `clear` method to `Family` to allow the removal of all label sets
  from a family. See [PR 85].

### Changed

- Move`Encode` trait from `prometheus_client::encoding::text` to `prometheus_client::encoding`. See [PR 83].

[PR 83]: https://github.com/prometheus/client_rust/pull/83
[PR 85]: https://github.com/prometheus/client_rust/pull/85

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
