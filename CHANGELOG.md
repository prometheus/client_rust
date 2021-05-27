# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.10.1] - unreleased
### Fixed
- Update to open-metrics-client-derive-text-encode v0.1.1 which handles keyword
  identifiers aka raw identifiers

  https://github.com/mxinden/rust-open-metrics-client/pull/16

## [0.10.0] - 2021-04-29
### Added
- Added `metrics::histogram::linear_buckets`.
  https://github.com/mxinden/rust-open-metrics-client/issues/13

### Changed
- Renamed `metrics::histogram::exponential_series` to
  `metrics::histogram::exponential_buckets`.
  https://github.com/mxinden/rust-open-metrics-client/issues/13
