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
//! assert_eq!("My counter.", family.help);
//! ```

// Include the `openmetrics_data_model` module, which is generated from `proto/openmetrics_data_model.proto`.
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
use std::vec::IntoIter;
use void::Void;

pub use openmetrics_data_model::*;
pub use prometheus_client_derive_proto_encode::*;

pub fn encode<M>(registry: &Registry<M>) -> openmetrics_data_model::MetricSet
where
    M: EncodeMetric,
{
    let mut metric_set = openmetrics_data_model::MetricSet::default();

    for (desc, metric) in registry.iter() {
        let mut family = openmetrics_data_model::MetricFamily::default();
        family.name = desc.name().to_string();
        family.r#type = {
            let metric_type: openmetrics_data_model::MetricType = metric.metric_type().into();
            metric_type as i32
        };
        if let Some(unit) = desc.unit() {
            family.unit = unit.as_str().to_string();
        }
        family.help = desc.help().to_string();
        family.metrics = metric
            .encode(desc.labels().encode().collect::<Vec<_>>())
            .collect::<Vec<_>>();

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
    type Iterator: Iterator<Item = openmetrics_data_model::Metric>;

    fn encode(&self, labels: Vec<openmetrics_data_model::Label>) -> Self::Iterator;

    fn metric_type(&self) -> MetricType;
}

impl EncodeMetric
    for Box<dyn EncodeMetric<Iterator = Box<dyn Iterator<Item = openmetrics_data_model::Metric>>>>
{
    type Iterator = Box<dyn Iterator<Item = openmetrics_data_model::Metric>>;

    fn encode(&self, labels: Vec<openmetrics_data_model::Label>) -> Self::Iterator {
        self.deref().encode(labels)
    }

    fn metric_type(&self) -> MetricType {
        self.deref().metric_type()
    }
}

pub trait SendEncodeMetric: EncodeMetric + Send {}

impl<T: EncodeMetric + Send> SendEncodeMetric for T {}

impl EncodeMetric
    for Box<
        dyn SendEncodeMetric<Iterator = Box<dyn Iterator<Item = openmetrics_data_model::Metric>>>,
    >
{
    type Iterator = Box<dyn Iterator<Item = openmetrics_data_model::Metric>>;

    fn encode(&self, labels: Vec<openmetrics_data_model::Label>) -> Self::Iterator {
        self.deref().encode(labels)
    }

    fn metric_type(&self) -> MetricType {
        self.deref().metric_type()
    }
}

pub trait EncodeLabels {
    type Iterator: Iterator<Item = openmetrics_data_model::Label>;

    fn encode(self) -> Self::Iterator;
}

impl<K: ToString, V: ToString> Into<openmetrics_data_model::Label> for &(K, V) {
    fn into(self) -> openmetrics_data_model::Label {
        let mut label = openmetrics_data_model::Label::default();
        label.name = self.0.to_string();
        label.value = self.1.to_string();
        label
    }
}

// TODO: Is this needed? We already have `&'a [T]` below.
impl<'a, T> EncodeLabels for &'a Vec<T>
where
    for<'b> &'b T: Into<openmetrics_data_model::Label>,
{
    type Iterator = Box<dyn Iterator<Item = openmetrics_data_model::Label> + 'a>;

    fn encode(self) -> Self::Iterator {
        Box::new(self.iter().map(|t| t.into()))
    }
}

impl<'a, T> EncodeLabels for &'a [T]
where
    for<'b> &'b T: Into<openmetrics_data_model::Label>,
{
    type Iterator = Box<dyn Iterator<Item = openmetrics_data_model::Label> + 'a>;

    fn encode(self) -> Self::Iterator {
        Box::new(self.iter().map(|t| t.into()))
    }
}

impl<'a> EncodeLabels for &'a Void {
    type Iterator = Box<dyn Iterator<Item = openmetrics_data_model::Label>>;

    fn encode(self) -> Self::Iterator {
        unreachable!()
    }
}

fn encode_exemplar<S, N>(exemplar: &Exemplar<S, N>) -> openmetrics_data_model::Exemplar
where
    N: Clone,
    for<'a> &'a S: EncodeLabels,
    f64: From<N>, // required because Exemplar.value is defined as `double` in protobuf
{
    let mut exemplar_proto = openmetrics_data_model::Exemplar::default();
    exemplar_proto.value = exemplar.value.clone().into();
    exemplar_proto.label = exemplar.label_set.encode().collect::<Vec<_>>();

    exemplar_proto
}

/////////////////////////////////////////////////////////////////////////////////
// Counter

pub trait EncodeCounterValue {
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
    type Iterator = std::iter::Once<openmetrics_data_model::Metric>;

    fn encode(&self, labels: Vec<openmetrics_data_model::Label>) -> Self::Iterator {
        let mut metric = encode_counter_with_maybe_exemplar(self.get(), None);
        metric.labels = labels;

        std::iter::once(metric)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

impl<'a, S, N, A> EncodeMetric for CounterWithExemplar<S, N, A>
where
    for<'b> &'b S: EncodeLabels,
    N: Clone + EncodeCounterValue,
    A: counter::Atomic<N>,
    f64: From<N>,
{
    type Iterator = std::iter::Once<openmetrics_data_model::Metric>;

    fn encode(&self, labels: Vec<openmetrics_data_model::Label>) -> Self::Iterator {
        let (value, exemplar) = self.get();

        let exemplar_proto = if let Some(e) = exemplar.as_ref() {
            Some(encode_exemplar(e))
        } else {
            None
        };

        let mut metric = encode_counter_with_maybe_exemplar(value.clone(), exemplar_proto);
        metric.labels = labels;

        std::iter::once(metric)
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
    let mut metric = openmetrics_data_model::Metric::default();

    metric.metric_points = {
        let mut metric_point = openmetrics_data_model::MetricPoint::default();
        metric_point.value = {
            let mut counter_value = openmetrics_data_model::CounterValue::default();
            counter_value.total = Some(value.encode());
            counter_value.exemplar = exemplar;

            Some(openmetrics_data_model::metric_point::Value::CounterValue(
                counter_value,
            ))
        };

        vec![metric_point]
    };

    metric
}

/////////////////////////////////////////////////////////////////////////////////
// Gauge

pub trait EncodeGaugeValue {
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

impl<'a, N, A> EncodeMetric for Gauge<N, A>
where
    N: EncodeGaugeValue,
    A: gauge::Atomic<N>,
{
    type Iterator = std::iter::Once<openmetrics_data_model::Metric>;

    fn encode(&self, labels: Vec<openmetrics_data_model::Label>) -> Self::Iterator {
        let mut metric = openmetrics_data_model::Metric::default();

        metric.metric_points = {
            let mut metric_point = openmetrics_data_model::MetricPoint::default();
            metric_point.value = {
                let mut gauge_value = openmetrics_data_model::GaugeValue::default();
                gauge_value.value = Some(self.get().encode());

                Some(openmetrics_data_model::metric_point::Value::GaugeValue(
                    gauge_value,
                ))
            };

            vec![metric_point]
        };

        metric.labels = labels;
        std::iter::once(metric)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Family

impl<S, M, C> EncodeMetric for Family<S, M, C>
where
    S: Clone + std::hash::Hash + Eq,
    for<'b> &'b S: EncodeLabels,
    M: EncodeMetric + TypedMetric,
    C: MetricConstructor<M>,
{
    type Iterator = IntoIter<openmetrics_data_model::Metric>;

    fn encode(&self, labels: Vec<openmetrics_data_model::Label>) -> Self::Iterator {
        self.read()
            .iter()
            .map(|(label_set, metric)| {
                let mut labels = labels.clone();
                labels.extend(label_set.encode());
                metric.encode(labels)
            })
            .flatten()
            // TODO: Ideally we would not have to collect into a vector here,
            // though we have to as we borrow from the `MutexGuard`. Once
            // https://github.com/prometheus/client_rust/pull/78/ merged, we
            // might be able to leverage `MutexGuard::map`.
            .collect::<Vec<_>>()
            .into_iter()
    }

    fn metric_type(&self) -> MetricType {
        M::TYPE
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Histogram

impl EncodeMetric for Histogram {
    type Iterator = std::iter::Once<openmetrics_data_model::Metric>;

    fn encode(&self, labels: Vec<openmetrics_data_model::Label>) -> Self::Iterator {
        let (sum, count, buckets) = self.get();
        // TODO: Would be better to use never type instead of `Void`.
        let mut metric = encode_histogram_with_maybe_exemplars::<Void>(sum, count, &buckets, None);
        metric.labels = labels;
        std::iter::once(metric)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

impl<S> EncodeMetric for HistogramWithExemplars<S>
where
    for<'b> &'b S: EncodeLabels,
{
    type Iterator = std::iter::Once<openmetrics_data_model::Metric>;

    fn encode(&self, labels: Vec<openmetrics_data_model::Label>) -> Self::Iterator {
        let inner = self.inner();
        let (sum, count, buckets) = inner.histogram.get();
        let mut metric =
            encode_histogram_with_maybe_exemplars(sum, count, &buckets, Some(&inner.exemplars));
        metric.labels = labels;
        std::iter::once(metric)
    }

    fn metric_type(&self) -> MetricType {
        Histogram::TYPE
    }
}

fn encode_histogram_with_maybe_exemplars<'a, S>(
    sum: f64,
    count: u64,
    buckets: &[(f64, u64)],
    exemplars: Option<&'a HashMap<usize, Exemplar<S, f64>>>,
) -> openmetrics_data_model::Metric
where
    for<'b> &'b S: EncodeLabels,
{
    let mut metric = openmetrics_data_model::Metric::default();

    metric.metric_points = {
        let mut metric_point = openmetrics_data_model::MetricPoint::default();
        metric_point.value = {
            let mut histogram_value = openmetrics_data_model::HistogramValue::default();
            histogram_value.sum = Some(openmetrics_data_model::histogram_value::Sum::DoubleValue(
                sum,
            ));
            histogram_value.count = count;

            let mut cummulative = 0;
            for (i, (upper_bound, count)) in buckets.iter().enumerate() {
                cummulative += count;
                let mut bucket = openmetrics_data_model::histogram_value::Bucket::default();
                bucket.count = cummulative;
                bucket.upper_bound = *upper_bound;
                bucket.exemplar = exemplars
                    .map(|es| es.get(&i))
                    .flatten()
                    .map(|exemplar| encode_exemplar(exemplar));
                histogram_value.buckets.push(bucket);
            }
            Some(openmetrics_data_model::metric_point::Value::HistogramValue(
                histogram_value,
            ))
        };

        vec![metric_point]
    };

    metric
}

/////////////////////////////////////////////////////////////////////////////////
// Info

impl<S> EncodeMetric for Info<S>
where
    for<'b> &'b S: EncodeLabels,
{
    type Iterator = std::iter::Once<openmetrics_data_model::Metric>;

    fn encode(&self, mut labels: Vec<openmetrics_data_model::Label>) -> Self::Iterator {
        let mut metric = openmetrics_data_model::Metric::default();

        metric.metric_points = {
            let mut metric_point = openmetrics_data_model::MetricPoint::default();
            metric_point.value = {
                labels.extend(self.0.encode());

                let mut info_value = openmetrics_data_model::InfoValue::default();
                info_value.info = labels;

                Some(openmetrics_data_model::metric_point::Value::InfoValue(
                    info_value,
                ))
            };

            vec![metric_point]
        };

        std::iter::once(metric)
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
