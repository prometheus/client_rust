# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
