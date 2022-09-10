//! Open Metrics protobuf implementation.
//!
//! ```
//! # use prometheus_client::encoding::proto::encode;
//! # use prometheus_client::metrics::counter::Counter;
//! # use prometheus_client::registry::Registry;
//! #
//! # // Create registry and counter and register the latter with the former.
//! # let mut registry = Registry::default();
//! # let counter: Counter = Counter::default();
//! # registry.register(
//! #   "my_counter",
//! #   "This is my counter",
//! #   counter.clone(),
//! # );
//! # counter.inc();
//! // Returns `MetricSet`, the top-level container type. Please refer to [openmetrics_data_model.proto](https://github.com/OpenObservability/OpenMetrics/blob/main/proto/openmetrics_data_model.proto) for details.
//! let metric_set = encode(&registry);
//!
//! let family = metric_set.metric_families.first().unwrap();
//! assert_eq!("my_counter", family.name);
//! assert_eq!("This is my counter.", family.help);
//! ```

// Allowing some lints here as the `openmetrics.rs` is an automatically generated file.
#[allow(missing_docs, clippy::derive_partial_eq_without_eq)]
/// Data models that are automatically generated from OpenMetrics protobuf
/// format.
pub mod openmetrics_data_model {
    include!(concat!(env!("OUT_DIR"), "/openmetrics.rs"));
}

use crate::metrics::counter::Counter;
use crate::metrics::exemplar::{CounterWithExemplar, Exemplar, HistogramWithExemplars};
use crate::metrics::family::{Family, MetricConstructor};
use crate::metrics::gauge::Gauge;
use crate::metrics::histogram::Histogram;
use crate::metrics::info::Info;
use crate::metrics::{counter, gauge, MetricType, TypedMetric};
use crate::registry::Registry;
use std::collections::HashMap;
use std::ops::Deref;
use void::Void;

pub use openmetrics_data_model::*;
pub use prometheus_client_derive_encode::*;

/// Encode the metrics registered with the provided [`Registry`] into MetricSet
/// using the OpenMetrics protobuf format.
pub fn encode<M>(registry: &Registry<M>) -> openmetrics_data_model::MetricSet
where
    M: EncodeMetric,
{
    let mut metric_set = openmetrics_data_model::MetricSet::default();

    for (desc, metric) in registry.iter() {
        let mut family = openmetrics_data_model::MetricFamily {
            name: desc.name().to_string(),
            r#type: {
                let metric_type: openmetrics_data_model::MetricType = metric.metric_type().into();
                metric_type as i32
            },
            unit: if let Some(unit) = desc.unit() {
                unit.as_str().to_string()
            } else {
                String::new()
            },
            help: desc.help().to_string(),
            ..Default::default()
        };

        let mut labels = vec![];
        desc.labels().encode(&mut labels);
        metric.encode(labels, &mut family.metrics);

        metric_set.metric_families.push(family);
    }

    metric_set
}

impl From<MetricType> for openmetrics_data_model::MetricType {
    fn from(m: MetricType) -> Self {
        match m {
            MetricType::Counter => openmetrics_data_model::MetricType::Counter,
            MetricType::Gauge => openmetrics_data_model::MetricType::Gauge,
            MetricType::Histogram => openmetrics_data_model::MetricType::Histogram,
            MetricType::Info => openmetrics_data_model::MetricType::Info,
            MetricType::Unknown => openmetrics_data_model::MetricType::Unknown,
        }
    }
}

/// Trait implemented by each metric type, e.g. [`Counter`], to implement its encoding.
pub trait EncodeMetric {
    /// Encode to OpenMetrics protobuf encoding.
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
        family: &mut Vec<openmetrics_data_model::Metric>,
    );

    /// The OpenMetrics metric type of the instance.
    fn metric_type(&self) -> MetricType;
}

impl EncodeMetric for Box<dyn EncodeMetric> {
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
        family: &mut Vec<openmetrics_data_model::Metric>,
    ) {
        self.deref().encode(labels, family)
    }

    fn metric_type(&self) -> MetricType {
        self.deref().metric_type()
    }
}

/// Trait combining [`EncodeMetric`] and [`Send`].
pub trait SendEncodeMetric: EncodeMetric + Send {}

impl<T: EncodeMetric + Send> SendEncodeMetric for T {}

/// Trait to implement its label encoding in the OpenMetrics protobuf format.
pub trait EncodeLabels {
    /// Encode the given instance into Labels in the OpenMetrics protobuf
    /// encoding.
    fn encode(&self, labels: &mut Vec<openmetrics_data_model::Label>);
}

impl<K: ToString, V: ToString> From<&(K, V)> for openmetrics_data_model::Label {
    fn from(kv: &(K, V)) -> Self {
        openmetrics_data_model::Label {
            name: kv.0.to_string(),
            value: kv.1.to_string(),
        }
    }
}

impl EncodeLabels for f64 {
    fn encode(&self, labels: &mut Vec<openmetrics_data_model::Label>) {
        labels.push(openmetrics_data_model::Label {
            name: self.to_string(),
            value: self.to_string(),
        })
    }
}

impl EncodeLabels for u64 {
    fn encode(&self, labels: &mut Vec<openmetrics_data_model::Label>) {
        labels.push(openmetrics_data_model::Label {
            name: self.to_string(),
            value: self.to_string(),
        })
    }
}

impl EncodeLabels for u32 {
    fn encode(&self, labels: &mut Vec<openmetrics_data_model::Label>) {
        labels.push(openmetrics_data_model::Label {
            name: self.to_string(),
            value: self.to_string(),
        })
    }
}

impl EncodeLabels for String {
    fn encode(&self, labels: &mut Vec<openmetrics_data_model::Label>) {
        labels.push(openmetrics_data_model::Label {
            name: self.clone(),
            value: self.clone(),
        })
    }
}

impl<T> EncodeLabels for Vec<T>
where
    for<'a> &'a T: Into<openmetrics_data_model::Label>,
{
    fn encode(&self, labels: &mut Vec<openmetrics_data_model::Label>) {
        self.as_slice().encode(labels);
    }
}

impl<T> EncodeLabels for [T]
where
    for<'a> &'a T: Into<openmetrics_data_model::Label>,
{
    fn encode(&self, labels: &mut Vec<openmetrics_data_model::Label>) {
        labels.extend(self.iter().map(|t| t.into()));
    }
}

impl EncodeLabels for Void {
    fn encode(&self, _labels: &mut Vec<openmetrics_data_model::Label>) {
        void::unreachable(*self);
    }
}

fn encode_exemplar<S, N>(exemplar: &Exemplar<S, N>) -> openmetrics_data_model::Exemplar
where
    N: Clone,
    S: EncodeLabels,
    f64: From<N>, // required because Exemplar.value is defined as `double` in protobuf
{
    let mut exemplar_proto = openmetrics_data_model::Exemplar {
        value: exemplar.value.clone().into(),
        ..Default::default()
    };
    exemplar.label_set.encode(&mut exemplar_proto.label);

    exemplar_proto
}

/////////////////////////////////////////////////////////////////////////////////
// Counter

/// Trait to implement its counter value encoding in the OpenMetrics protobuf
/// format.
pub trait EncodeCounterValue {
    /// Encode the given instance into counter value in the OpenMetrics protobuf
    /// encoding.
    fn encode(&self) -> openmetrics_data_model::counter_value::Total;
}

impl EncodeCounterValue for u64 {
    fn encode(&self) -> openmetrics_data_model::counter_value::Total {
        openmetrics_data_model::counter_value::Total::IntValue(*self)
    }
}

impl EncodeCounterValue for f64 {
    fn encode(&self) -> openmetrics_data_model::counter_value::Total {
        openmetrics_data_model::counter_value::Total::DoubleValue(*self)
    }
}

impl<N, A> EncodeMetric for Counter<N, A>
where
    N: EncodeCounterValue,
    A: counter::Atomic<N>,
{
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
        family: &mut Vec<openmetrics_data_model::Metric>,
    ) {
        let mut metric = encode_counter_with_maybe_exemplar(self.get(), None);
        metric.labels = labels;

        family.push(metric);
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

impl<S, N, A> EncodeMetric for CounterWithExemplar<S, N, A>
where
    S: EncodeLabels,
    N: Clone + EncodeCounterValue,
    A: counter::Atomic<N>,
    f64: From<N>,
{
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
        family: &mut Vec<openmetrics_data_model::Metric>,
    ) {
        let (value, exemplar) = self.get();
        let exemplar_proto = exemplar.as_ref().map(|e| encode_exemplar(e));
        let mut metric = encode_counter_with_maybe_exemplar(value, exemplar_proto);
        metric.labels = labels;

        family.push(metric);
    }

    fn metric_type(&self) -> MetricType {
        Counter::<N, A>::TYPE
    }
}

fn encode_counter_with_maybe_exemplar<N>(
    value: N,
    exemplar: Option<openmetrics_data_model::Exemplar>,
) -> openmetrics_data_model::Metric
where
    N: EncodeCounterValue,
{
    openmetrics_data_model::Metric {
        metric_points: {
            let metric_point = openmetrics_data_model::MetricPoint {
                value: {
                    Some(openmetrics_data_model::metric_point::Value::CounterValue(
                        openmetrics_data_model::CounterValue {
                            total: Some(value.encode()),
                            exemplar,
                            ..Default::default()
                        },
                    ))
                },
                ..Default::default()
            };

            vec![metric_point]
        },
        ..Default::default()
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Gauge

/// Trait to implement its gauge value encoding in the OpenMetrics protobuf
/// format.
pub trait EncodeGaugeValue {
    /// Encode the given instance into gauge value in the OpenMetrics protobuf
    /// encoding.
    fn encode(&self) -> openmetrics_data_model::gauge_value::Value;
}

// GaugeValue.int_value is defined as `int64` in protobuf
impl EncodeGaugeValue for i64 {
    fn encode(&self) -> openmetrics_data_model::gauge_value::Value {
        openmetrics_data_model::gauge_value::Value::IntValue(*self)
    }
}

impl EncodeGaugeValue for u64 {
    fn encode(&self) -> openmetrics_data_model::gauge_value::Value {
        openmetrics_data_model::gauge_value::Value::IntValue(*self as i64)
    }
}

impl EncodeGaugeValue for f64 {
    fn encode(&self) -> openmetrics_data_model::gauge_value::Value {
        openmetrics_data_model::gauge_value::Value::DoubleValue(*self)
    }
}

impl<N, A> EncodeMetric for Gauge<N, A>
where
    N: EncodeGaugeValue,
    A: gauge::Atomic<N>,
{
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
        family: &mut Vec<openmetrics_data_model::Metric>,
    ) {
        let metric = openmetrics_data_model::Metric {
            metric_points: {
                let metric_point = openmetrics_data_model::MetricPoint {
                    value: {
                        Some(openmetrics_data_model::metric_point::Value::GaugeValue(
                            openmetrics_data_model::GaugeValue {
                                value: Some(self.get().encode()),
                            },
                        ))
                    },
                    ..Default::default()
                };

                vec![metric_point]
            },
            labels,
        };

        family.push(metric)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Family

impl<S, M, C> EncodeMetric for Family<S, M, C>
where
    S: EncodeLabels + Clone + std::hash::Hash + Eq,
    M: EncodeMetric + TypedMetric,
    C: MetricConstructor<M>,
{
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
        family: &mut Vec<openmetrics_data_model::Metric>,
    ) {
        for (label_set, metric) in self.read().iter() {
            let mut labels = labels.clone();
            label_set.encode(&mut labels);
            metric.encode(labels, family)
        }
    }

    fn metric_type(&self) -> MetricType {
        M::TYPE
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Histogram

impl EncodeMetric for Histogram {
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
        family: &mut Vec<openmetrics_data_model::Metric>,
    ) {
        let (sum, count, buckets) = self.get();
        // TODO: Would be better to use never type instead of `Void`.
        let mut metric = encode_histogram_with_maybe_exemplars::<Void>(sum, count, &buckets, None);
        metric.labels = labels;

        family.push(metric)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

impl<S> EncodeMetric for HistogramWithExemplars<S>
where
    S: EncodeLabels,
{
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
        family: &mut Vec<openmetrics_data_model::Metric>,
    ) {
        let inner = self.inner();
        let (sum, count, buckets) = inner.histogram.get();
        let mut metric =
            encode_histogram_with_maybe_exemplars(sum, count, &buckets, Some(&inner.exemplars));
        metric.labels = labels;

        family.push(metric)
    }

    fn metric_type(&self) -> MetricType {
        Histogram::TYPE
    }
}

fn encode_histogram_with_maybe_exemplars<S>(
    sum: f64,
    count: u64,
    buckets: &[(f64, u64)],
    exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
) -> openmetrics_data_model::Metric
where
    S: EncodeLabels,
{
    openmetrics_data_model::Metric {
        metric_points: {
            let metric_point = openmetrics_data_model::MetricPoint {
                value: {
                    let mut histogram_value = openmetrics_data_model::HistogramValue {
                        sum: Some(openmetrics_data_model::histogram_value::Sum::DoubleValue(
                            sum,
                        )),
                        count,
                        ..Default::default()
                    };

                    let mut cummulative = 0;
                    for (i, (upper_bound, count)) in buckets.iter().enumerate() {
                        cummulative += count;
                        let bucket = openmetrics_data_model::histogram_value::Bucket {
                            count: cummulative,
                            upper_bound: *upper_bound,
                            exemplar: exemplars
                                .and_then(|es| es.get(&i))
                                .map(|exemplar| encode_exemplar(exemplar)),
                        };
                        histogram_value.buckets.push(bucket);
                    }
                    Some(openmetrics_data_model::metric_point::Value::HistogramValue(
                        histogram_value,
                    ))
                },
                ..Default::default()
            };

            vec![metric_point]
        },
        ..Default::default()
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Info

impl<S> EncodeMetric for Info<S>
where
    S: EncodeLabels,
{
    fn encode(
        &self,
        mut labels: Vec<openmetrics_data_model::Label>,
        family: &mut Vec<openmetrics_data_model::Metric>,
    ) {
        let metric = openmetrics_data_model::Metric {
            metric_points: {
                let metric_point = openmetrics_data_model::MetricPoint {
                    value: {
                        self.0.encode(&mut labels);

                        Some(openmetrics_data_model::metric_point::Value::InfoValue(
                            openmetrics_data_model::InfoValue { info: labels },
                        ))
                    },
                    ..Default::default()
                };

                vec![metric_point]
            },
            ..Default::default()
        };

        family.push(metric);
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::counter::Counter;
    use crate::metrics::exemplar::{CounterWithExemplar, HistogramWithExemplars};
    use crate::metrics::family::Family;
    use crate::metrics::gauge::Gauge;
    use crate::metrics::histogram::{exponential_buckets, Histogram};
    use crate::metrics::info::Info;
    use crate::registry::Unit;
    use std::borrow::Cow;
    use std::sync::atomic::AtomicI64;

    #[test]
    fn encode_counter_int() {
        let counter: Counter = Counter::default();
        let mut registry = Registry::default();
        registry.register("my_counter", "My counter", counter.clone());
        counter.inc();

        let metric_set = encode(&registry);

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_counter", family.name);
        assert_eq!("My counter.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Counter as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(metric_set) {
            openmetrics_data_model::metric_point::Value::CounterValue(value) => {
                let expected = openmetrics_data_model::counter_value::Total::IntValue(1);
                assert_eq!(Some(expected), value.total);
                assert_eq!(None, value.exemplar);
                assert_eq!(None, value.created);
            }
            _ => assert!(false, "wrong value type"),
        }
    }

    #[test]
    fn encode_counter_double() {
        // Using `f64`
        let counter: Counter<f64> = Counter::default();
        let mut registry = Registry::default();
        registry.register("my_counter", "My counter", counter.clone());
        counter.inc();

        let metric_set = encode(&registry);

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_counter", family.name);
        assert_eq!("My counter.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Counter as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(metric_set) {
            openmetrics_data_model::metric_point::Value::CounterValue(value) => {
                // The counter should be encoded  as `DoubleValue`
                let expected = openmetrics_data_model::counter_value::Total::DoubleValue(1.0);
                assert_eq!(Some(expected), value.total);
                assert_eq!(None, value.exemplar);
                assert_eq!(None, value.created);
            }
            _ => assert!(false, "wrong value type"),
        }
    }

    #[test]
    fn encode_counter_with_unit() {
        let mut registry = Registry::default();
        let counter: Counter = Counter::default();
        registry.register_with_unit("my_counter", "My counter", Unit::Seconds, counter.clone());

        let metric_set = encode(&registry);

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_counter", family.name);
        assert_eq!("My counter.", family.help);
        assert_eq!("seconds", family.unit);
    }

    #[test]
    fn encode_counter_with_exemplar() {
        let mut registry = Registry::default();

        let counter_with_exemplar: CounterWithExemplar<Vec<(String, f64)>, f64> =
            CounterWithExemplar::default();
        registry.register(
            "my_counter_with_exemplar",
            "My counter with exemplar",
            counter_with_exemplar.clone(),
        );

        counter_with_exemplar.inc_by(1.0, Some(vec![("user_id".to_string(), 42.0)]));

        let metric_set = encode(&registry);

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_counter_with_exemplar", family.name);
        assert_eq!("My counter with exemplar.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Counter as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(metric_set) {
            openmetrics_data_model::metric_point::Value::CounterValue(value) => {
                // The counter should be encoded  as `DoubleValue`
                let expected = openmetrics_data_model::counter_value::Total::DoubleValue(1.0);
                assert_eq!(Some(expected), value.total);

                let exemplar = value.exemplar.as_ref().unwrap();
                assert_eq!(1.0, exemplar.value);

                let expected_label = {
                    let mut label = openmetrics_data_model::Label::default();
                    label.name = "user_id".to_string();
                    label.value = "42".to_string();
                    label
                };
                assert_eq!(vec![expected_label], exemplar.label);
            }
            _ => assert!(false, "wrong value type"),
        }
    }

    #[test]
    fn encode_gauge() {
        let mut registry = Registry::default();
        let gauge = Gauge::<i64, AtomicI64>::default();
        registry.register("my_gauge", "My gauge", gauge.clone());
        gauge.inc();

        let metric_set = encode(&registry);
        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_gauge", family.name);
        assert_eq!("My gauge.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Gauge as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(metric_set) {
            openmetrics_data_model::metric_point::Value::GaugeValue(value) => {
                let expected = openmetrics_data_model::gauge_value::Value::IntValue(1);
                assert_eq!(Some(expected), value.value);
            }
            _ => assert!(false, "wrong value type"),
        }
    }

    #[test]
    fn encode_counter_family() {
        let mut registry = Registry::default();
        let family = Family::<Vec<(String, String)>, Counter>::default();
        registry.register("my_counter_family", "My counter family", family.clone());

        family
            .get_or_create(&vec![
                ("method".to_string(), "GET".to_string()),
                ("status".to_string(), "200".to_string()),
            ])
            .inc();

        let metric_set = encode(&registry);

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_counter_family", family.name);
        assert_eq!("My counter family.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Counter as i32,
            extract_metric_type(&metric_set)
        );

        let metric = family.metrics.first().unwrap();
        assert_eq!(2, metric.labels.len());
        assert_eq!("method", metric.labels[0].name);
        assert_eq!("GET", metric.labels[0].value);
        assert_eq!("status", metric.labels[1].name);
        assert_eq!("200", metric.labels[1].value);

        match extract_metric_point_value(metric_set) {
            openmetrics_data_model::metric_point::Value::CounterValue(value) => {
                let expected = openmetrics_data_model::counter_value::Total::IntValue(1);
                assert_eq!(Some(expected), value.total);
                assert_eq!(None, value.exemplar);
                assert_eq!(None, value.created);
            }
            _ => assert!(false, "wrong value type"),
        }
    }

    #[test]
    fn encode_counter_family_with_prefix_with_label() {
        let mut registry = Registry::default();
        let sub_registry = registry.sub_registry_with_prefix("my_prefix");
        let sub_sub_registry = sub_registry
            .sub_registry_with_label((Cow::Borrowed("my_key"), Cow::Borrowed("my_value")));
        let family = Family::<Vec<(String, String)>, Counter>::default();
        sub_sub_registry.register("my_counter_family", "My counter family", family.clone());

        family
            .get_or_create(&vec![
                ("method".to_string(), "GET".to_string()),
                ("status".to_string(), "200".to_string()),
            ])
            .inc();

        let metric_set = encode(&registry);

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_prefix_my_counter_family", family.name);
        assert_eq!("My counter family.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Counter as i32,
            extract_metric_type(&metric_set)
        );

        let metric = family.metrics.first().unwrap();
        assert_eq!(3, metric.labels.len());
        assert_eq!("my_key", metric.labels[0].name);
        assert_eq!("my_value", metric.labels[0].value);
        assert_eq!("method", metric.labels[1].name);
        assert_eq!("GET", metric.labels[1].value);
        assert_eq!("status", metric.labels[2].name);
        assert_eq!("200", metric.labels[2].value);

        match extract_metric_point_value(metric_set) {
            openmetrics_data_model::metric_point::Value::CounterValue(value) => {
                let expected = openmetrics_data_model::counter_value::Total::IntValue(1);
                assert_eq!(Some(expected), value.total);
                assert_eq!(None, value.exemplar);
                assert_eq!(None, value.created);
            }
            _ => assert!(false, "wrong value type"),
        }
    }

    #[test]
    fn encode_histogram() {
        let mut registry = Registry::default();
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);

        let metric_set = encode(&registry);

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_histogram", family.name);
        assert_eq!("My histogram.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Histogram as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(metric_set) {
            openmetrics_data_model::metric_point::Value::HistogramValue(value) => {
                assert_eq!(
                    Some(openmetrics_data_model::histogram_value::Sum::DoubleValue(
                        1.0
                    )),
                    value.sum
                );
                assert_eq!(1, value.count);
                assert_eq!(11, value.buckets.len());
            }
            _ => assert!(false, "wrong value type"),
        }
    }

    #[test]
    fn encode_histogram_with_exemplars() {
        let mut registry = Registry::default();
        let histogram = HistogramWithExemplars::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0, Some(vec![("user_id".to_string(), 42u64)]));

        let metric_set = encode(&registry);

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_histogram", family.name);
        assert_eq!("My histogram.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Histogram as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(metric_set) {
            openmetrics_data_model::metric_point::Value::HistogramValue(value) => {
                let exemplar = value.buckets.first().unwrap().exemplar.as_ref().unwrap();
                assert_eq!(1.0, exemplar.value);

                let expected_label = {
                    let mut label = openmetrics_data_model::Label::default();
                    label.name = "user_id".to_string();
                    label.value = "42".to_string();
                    label
                };
                assert_eq!(vec![expected_label], exemplar.label);
            }
            _ => assert!(false, "wrong value type"),
        }
    }

    #[test]
    fn encode_family_counter_histogram() {
        let mut registry = Registry::<Box<dyn EncodeMetric>>::default();

        let counter_family = Family::<Vec<(String, String)>, Counter>::default();
        let histogram_family =
            Family::<Vec<(String, String)>, Histogram>::new_with_constructor(|| {
                Histogram::new(exponential_buckets(1.0, 2.0, 10))
            });

        registry.register("my_counter", "My counter", Box::new(counter_family.clone()));
        registry.register(
            "my_histogram",
            "My histogram",
            Box::new(histogram_family.clone()),
        );

        counter_family
            .get_or_create(&vec![("path".to_string(), "/".to_string())])
            .inc();

        histogram_family
            .get_or_create(&vec![("path".to_string(), "/".to_string())])
            .observe(1.0);

        let metric_set = encode(&registry);
        assert_eq!("my_counter", metric_set.metric_families[0].name);
        assert_eq!("my_histogram", metric_set.metric_families[1].name);
    }

    #[test]
    fn encode_family_and_counter_and_histogram() {
        let mut registry = Registry::<Box<dyn EncodeMetric>>::default();

        // Family
        let counter_family = Family::<Vec<(String, String)>, Counter>::default();
        let histogram_family =
            Family::<Vec<(String, String)>, Histogram>::new_with_constructor(|| {
                Histogram::new(exponential_buckets(1.0, 2.0, 10))
            });

        registry.register(
            "my_family_counter",
            "My counter",
            Box::new(counter_family.clone()),
        );
        registry.register(
            "my_family_histogram",
            "My histogram",
            Box::new(histogram_family.clone()),
        );

        counter_family
            .get_or_create(&vec![("path".to_string(), "/".to_string())])
            .inc();

        histogram_family
            .get_or_create(&vec![("path".to_string(), "/".to_string())])
            .observe(1.0);

        // Counter
        let counter: Counter = Counter::default();
        registry.register("my_counter", "My counter", Box::new(counter.clone()));
        counter.inc();

        // Histogram
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", Box::new(histogram.clone()));
        histogram.observe(1.0);

        let metric_set = encode(&registry);
        assert_eq!("my_family_counter", metric_set.metric_families[0].name);
        assert_eq!("my_family_histogram", metric_set.metric_families[1].name);
    }

    #[test]
    fn encode_info() {
        let mut registry = Registry::default();
        let info = Info::new(vec![("os".to_string(), "GNU/linux".to_string())]);
        registry.register("my_info_metric", "My info metric", info);

        let metric_set = encode(&registry);

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_info_metric", family.name);
        assert_eq!("My info metric.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Info as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(metric_set) {
            openmetrics_data_model::metric_point::Value::InfoValue(value) => {
                assert_eq!(1, value.info.len());

                let info = value.info.first().unwrap();
                assert_eq!("os", info.name);
                assert_eq!("GNU/linux", info.value);
            }
            _ => assert!(false, "wrong value type"),
        }
    }

    fn extract_metric_type(metric_set: &openmetrics_data_model::MetricSet) -> i32 {
        let family = metric_set.metric_families.first().unwrap();
        family.r#type
    }

    fn extract_metric_point_value(
        metric_set: openmetrics_data_model::MetricSet,
    ) -> openmetrics_data_model::metric_point::Value {
        let metric = metric_set
            .metric_families
            .first()
            .unwrap()
            .metrics
            .first()
            .unwrap();

        metric
            .metric_points
            .first()
            .unwrap()
            .value
            .as_ref()
            .unwrap()
            .clone()
    }
}
