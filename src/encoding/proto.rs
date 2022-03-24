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
use crate::registry::{Registry, Unit};
use std::collections::HashMap;
use std::ops::Deref;

pub fn encode<M>(registry: &Registry<M>) -> openmetrics_data_model::MetricSet
where
    M: EncodeMetric,
{
    // MetricSet
    let mut metric_set = openmetrics_data_model::MetricSet::default();

    for (desc, metric) in registry.iter() {
        // MetricFamily
        let mut family = openmetrics_data_model::MetricFamily::default();
        // MetricFamily.name
        family.name = desc.name().to_string();
        // MetricFamily.type
        family.r#type = {
            let metric_type: openmetrics_data_model::MetricType = metric.metric_type().into();
            metric_type as i32
        };
        // MetricFamily.unit
        if let Some(unit) = desc.unit() {
            family.unit = match unit {
                Unit::Amperes => "amperes",
                Unit::Bytes => "bytes",
                Unit::Celsius => "celsius",
                Unit::Grams => "grams",
                Unit::Joules => "joules",
                Unit::Meters => "meters",
                Unit::Ratios => "ratios",
                Unit::Seconds => "seconds",
                Unit::Volts => "volts",
                Unit::Other(other) => other.as_str(),
            }
            .to_string();
        }
        // MetricFamily.help
        family.help = desc.help().to_string();
        // MetricFamily.Metric
        family.metrics = metric.encode(desc.labels().encode());
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
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
    ) -> Vec<openmetrics_data_model::Metric>;

    fn metric_type(&self) -> MetricType;
}

impl EncodeMetric for Box<dyn EncodeMetric> {
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
    ) -> Vec<openmetrics_data_model::Metric> {
        self.deref().encode(labels)
    }

    fn metric_type(&self) -> MetricType {
        self.deref().metric_type()
    }
}

pub trait SendEncodeMetric: EncodeMetric + Send {}

impl<T: EncodeMetric + Send> SendEncodeMetric for T {}

impl EncodeMetric for Box<dyn SendEncodeMetric> {
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
    ) -> Vec<openmetrics_data_model::Metric> {
        self.deref().encode(labels)
    }

    fn metric_type(&self) -> MetricType {
        self.deref().metric_type()
    }
}

pub trait EncodeLabel {
    fn encode(&self) -> Vec<openmetrics_data_model::Label>;
}

impl<K: ToString, V: ToString> EncodeLabel for (K, V) {
    fn encode(&self) -> Vec<openmetrics_data_model::Label> {
        let mut label = openmetrics_data_model::Label::default();
        label.name = self.0.to_string();
        label.value = self.1.to_string();
        vec![label]
    }
}

impl<T: EncodeLabel> EncodeLabel for Vec<T> {
    fn encode(&self) -> Vec<openmetrics_data_model::Label> {
        let mut label = vec![];
        for t in self {
            label.append(&mut t.encode());
        }
        label
    }
}

impl<T: EncodeLabel> EncodeLabel for &[T] {
    fn encode(&self) -> Vec<openmetrics_data_model::Label> {
        let mut label = vec![];
        for t in self.iter() {
            label.append(&mut t.encode());
        }
        label
    }
}

impl EncodeLabel for () {
    fn encode(&self) -> Vec<openmetrics_data_model::Label> {
        vec![]
    }
}

fn encode_exemplar<S, N>(exemplar: &Exemplar<S, N>) -> openmetrics_data_model::Exemplar
where
    N: Clone,
    S: EncodeLabel,
    f64: From<N>, // required because Exemplar.value is defined as `double` in protobuf
{
    let mut exemplar_proto = openmetrics_data_model::Exemplar::default();
    exemplar_proto.value = exemplar.value.clone().into();
    exemplar_proto.label = exemplar.label_set.encode();

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
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
    ) -> Vec<openmetrics_data_model::Metric> {
        let mut metric = encode_counter_with_maybe_exemplar(self.get(), None);
        metric.labels = labels;

        vec![metric]
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

impl<S, N, A> EncodeMetric for CounterWithExemplar<S, N, A>
where
    S: EncodeLabel,
    N: Clone + EncodeCounterValue,
    A: counter::Atomic<N>,
    f64: From<N>,
{
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
    ) -> Vec<openmetrics_data_model::Metric> {
        let (value, exemplar) = self.get();

        let exemplar_proto = if let Some(e) = exemplar.as_ref() {
            Some(encode_exemplar(e))
        } else {
            None
        };

        let mut metric = encode_counter_with_maybe_exemplar(value.clone(), exemplar_proto);
        metric.labels = labels;

        vec![metric]
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
    ) -> Vec<openmetrics_data_model::Metric> {
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
        vec![metric]
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

/////////////////////////////////////////////////////////////////////////////////
// Family

impl<S, M, C> EncodeMetric for Family<S, M, C>
where
    S: Clone + std::hash::Hash + Eq + EncodeLabel,
    M: EncodeMetric + TypedMetric,
    C: MetricConstructor<M>,
{
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
    ) -> Vec<openmetrics_data_model::Metric> {
        let mut metrics = vec![];

        let guard = self.read();
        for (label_set, metric) in guard.iter() {
            let mut label = label_set.encode();
            label.append(&mut labels.clone());
            metrics.extend(metric.encode(label));
        }

        metrics
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
    ) -> Vec<openmetrics_data_model::Metric> {
        let (sum, count, buckets) = self.get();
        // TODO: Would be better to use never type instead of `()`.
        let mut metric = encode_histogram_with_maybe_exemplars::<()>(sum, count, &buckets, None);
        metric.labels = labels;
        vec![metric]
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

impl<S> EncodeMetric for HistogramWithExemplars<S>
where
    S: EncodeLabel,
{
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
    ) -> Vec<openmetrics_data_model::Metric> {
        let inner = self.inner();
        let (sum, count, buckets) = inner.histogram.get();
        let mut metric =
            encode_histogram_with_maybe_exemplars(sum, count, &buckets, Some(&inner.exemplars));
        metric.labels = labels;
        vec![metric]
    }

    fn metric_type(&self) -> MetricType {
        Histogram::TYPE
    }
}

fn encode_histogram_with_maybe_exemplars<S: EncodeLabel>(
    sum: f64,
    count: u64,
    buckets: &[(f64, u64)],
    exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
) -> openmetrics_data_model::Metric {
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
    S: EncodeLabel,
{
    fn encode(
        &self,
        labels: Vec<openmetrics_data_model::Label>,
    ) -> Vec<openmetrics_data_model::Metric> {
        let mut metric = openmetrics_data_model::Metric::default();

        metric.metric_points = {
            let mut metric_point = openmetrics_data_model::MetricPoint::default();
            metric_point.value = {
                let mut label = self.0.encode();
                label.append(&mut labels.clone());

                let mut info_value = openmetrics_data_model::InfoValue::default();
                info_value.info = label;

                Some(openmetrics_data_model::metric_point::Value::InfoValue(
                    info_value,
                ))
            };

            vec![metric_point]
        };

        vec![metric]
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
    fn test_encode() {
        let mut registry: Registry<Box<dyn EncodeMetric>> = Registry::default();

        let counter: Counter = Counter::default();
        registry.register_with_unit(
            "my_counter",
            "My counter",
            Unit::Seconds,
            Box::new(counter.clone()),
        );
        counter.inc();

        let family = Family::<Vec<(String, String)>, Counter>::default();
        let sub_registry =
            registry.sub_registry_with_label((Cow::Borrowed("my_key"), Cow::Borrowed("my_value")));
        sub_registry.register(
            "my_counter_family",
            "My counter family",
            Box::new(family.clone()),
        );
        family
            .get_or_create(&vec![
                ("method".to_string(), "GET".to_string()),
                ("status".to_string(), "200".to_string()),
            ])
            .inc();
        family
            .get_or_create(&vec![
                ("method".to_string(), "POST".to_string()),
                ("status".to_string(), "503".to_string()),
            ])
            .inc();

        println!("{:?}", encode(&registry));
    }

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
    fn encode_counter_with_exemplar() {
        let mut registry = Registry::default();

        let counter_with_exemplar: CounterWithExemplar<(String, f64), f64> =
            CounterWithExemplar::default();
        registry.register_with_unit(
            "my_counter_with_exemplar",
            "My counter with exemplar",
            Unit::Seconds,
            counter_with_exemplar.clone(),
        );

        counter_with_exemplar.inc_by(1.0, Some(("user_id".to_string(), 42.0)));

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
        histogram.observe(1.0, Some(("user_id".to_string(), 42u64)));

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
