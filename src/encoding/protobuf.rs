//! Open Metrics protobuf implementation.
//!
//! ```
//! # use prometheus_client::encoding::protobuf::encode;
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
//! let metric_set = encode(&registry).unwrap();
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

use std::collections::HashMap;

use crate::metrics::exemplar::Exemplar;
use crate::metrics::MetricType;
use crate::registry::Registry;

use super::{EncodeCounterValue, EncodeExemplarValue, EncodeGaugeValue, EncodeLabelSet};

/// Encode the metrics registered with the provided [`Registry`] into MetricSet
/// using the OpenMetrics protobuf format.
pub fn encode(registry: &Registry) -> Result<openmetrics_data_model::MetricSet, std::fmt::Error> {
    Ok(openmetrics_data_model::MetricSet {
        metric_families: registry
            .iter_metrics()
            .map(|(desc, metric)| encode_metric(desc, metric.as_ref()))
            .chain(
                registry
                    .iter_collectors()
                    .map(|(desc, metric)| encode_metric(desc.as_ref(), metric.as_ref())),
            )
            .collect::<Result<_, std::fmt::Error>>()?,
    })
}

fn encode_metric(
    desc: &crate::registry::Descriptor,
    metric: &(impl super::EncodeMetric + ?Sized),
) -> Result<openmetrics_data_model::MetricFamily, std::fmt::Error> {
    let mut family = openmetrics_data_model::MetricFamily {
        name: desc.name().to_string(),
        r#type: {
            let metric_type: openmetrics_data_model::MetricType =
                super::EncodeMetric::metric_type(metric).into();
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
    desc.labels().encode(
        LabelSetEncoder {
            labels: &mut labels,
        }
        .into(),
    )?;

    let encoder = MetricEncoder {
        family: &mut family.metrics,
        metric_type: super::EncodeMetric::metric_type(metric),
        labels,
    };

    super::EncodeMetric::encode(metric, encoder.into())?;

    Ok(family)
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

/// Encoder for protobuf encoding.
///
/// This is an inner type for [`super::MetricEncoder`].
#[derive(Debug)]
pub(crate) struct MetricEncoder<'a> {
    /// OpenMetrics metric type of the metric.
    metric_type: MetricType,
    /// Vector of OpenMetrics metrics to which encoded metrics are added.
    family: &'a mut Vec<openmetrics_data_model::Metric>,
    /// Labels to be added to each metric.
    labels: Vec<openmetrics_data_model::Label>,
}

impl<'a> MetricEncoder<'a> {
    pub fn encode_counter<
        S: EncodeLabelSet,
        CounterValue: EncodeCounterValue,
        ExemplarValue: EncodeExemplarValue,
    >(
        &mut self,
        v: &CounterValue,
        exemplar: Option<&Exemplar<S, ExemplarValue>>,
    ) -> Result<(), std::fmt::Error> {
        let mut value = openmetrics_data_model::counter_value::Total::IntValue(0);
        let mut e = CounterValueEncoder { value: &mut value }.into();
        v.encode(&mut e)?;

        self.family.push(openmetrics_data_model::Metric {
            labels: self.labels.clone(),
            metric_points: vec![openmetrics_data_model::MetricPoint {
                value: Some(openmetrics_data_model::metric_point::Value::CounterValue(
                    openmetrics_data_model::CounterValue {
                        total: Some(value),
                        exemplar: exemplar.map(|e| e.try_into()).transpose()?,
                        ..Default::default()
                    },
                )),
                ..Default::default()
            }],
        });

        Ok(())
    }

    pub fn encode_gauge<GaugeValue: EncodeGaugeValue>(
        &mut self,
        v: &GaugeValue,
    ) -> Result<(), std::fmt::Error> {
        let mut value = openmetrics_data_model::gauge_value::Value::IntValue(0);
        let mut e = GaugeValueEncoder { value: &mut value }.into();
        v.encode(&mut e)?;

        self.family.push(openmetrics_data_model::Metric {
            labels: self.labels.clone(),
            metric_points: vec![openmetrics_data_model::MetricPoint {
                value: Some(openmetrics_data_model::metric_point::Value::GaugeValue(
                    openmetrics_data_model::GaugeValue { value: Some(value) },
                )),
                ..Default::default()
            }],
        });

        Ok(())
    }

    pub fn encode_info(
        &mut self,
        label_set: &impl super::EncodeLabelSet,
    ) -> Result<(), std::fmt::Error> {
        let mut info_labels = vec![];
        label_set.encode(
            LabelSetEncoder {
                labels: &mut info_labels,
            }
            .into(),
        )?;

        self.family.push(openmetrics_data_model::Metric {
            labels: self.labels.clone(),
            metric_points: vec![openmetrics_data_model::MetricPoint {
                value: Some(openmetrics_data_model::metric_point::Value::InfoValue(
                    openmetrics_data_model::InfoValue { info: info_labels },
                )),
                ..Default::default()
            }],
        });

        Ok(())
    }

    pub fn encode_family<'b, S: EncodeLabelSet>(
        &'b mut self,
        label_set: &S,
    ) -> Result<MetricEncoder<'b>, std::fmt::Error> {
        let mut labels = self.labels.clone();
        label_set.encode(
            LabelSetEncoder {
                labels: &mut labels,
            }
            .into(),
        )?;

        Ok(MetricEncoder {
            metric_type: self.metric_type,
            family: self.family,
            labels,
        })
    }

    pub fn encode_histogram<S: EncodeLabelSet>(
        &mut self,
        sum: f64,
        count: u64,
        buckets: &[(f64, u64)],
        exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
    ) -> Result<(), std::fmt::Error> {
        let buckets = buckets
            .iter()
            .enumerate()
            .map(|(i, (upper_bound, count))| {
                Ok(openmetrics_data_model::histogram_value::Bucket {
                    upper_bound: *upper_bound,
                    count: *count,
                    exemplar: exemplars
                        .and_then(|exemplars| exemplars.get(&i).map(|exemplar| exemplar.try_into()))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, std::fmt::Error>>()?;

        self.family.push(openmetrics_data_model::Metric {
            labels: self.labels.clone(),
            metric_points: vec![openmetrics_data_model::MetricPoint {
                value: Some(openmetrics_data_model::metric_point::Value::HistogramValue(
                    openmetrics_data_model::HistogramValue {
                        count,
                        created: None,
                        buckets,
                        sum: Some(openmetrics_data_model::histogram_value::Sum::DoubleValue(
                            sum,
                        )),
                    },
                )),
                ..Default::default()
            }],
        });

        Ok(())
    }
}

impl<S: EncodeLabelSet, V: EncodeExemplarValue> TryFrom<&Exemplar<S, V>>
    for openmetrics_data_model::Exemplar
{
    type Error = std::fmt::Error;

    fn try_from(exemplar: &Exemplar<S, V>) -> Result<Self, Self::Error> {
        let mut value = f64::default();
        exemplar
            .value
            .encode(ExemplarValueEncoder { value: &mut value }.into())?;

        let mut labels = vec![];
        exemplar.label_set.encode(
            LabelSetEncoder {
                labels: &mut labels,
            }
            .into(),
        )?;

        Ok(openmetrics_data_model::Exemplar {
            value,
            timestamp: Default::default(),
            label: labels,
        })
    }
}

#[derive(Debug)]
pub(crate) struct GaugeValueEncoder<'a> {
    value: &'a mut openmetrics_data_model::gauge_value::Value,
}

impl<'a> GaugeValueEncoder<'a> {
    pub fn encode_i64(&mut self, v: i64) -> Result<(), std::fmt::Error> {
        *self.value = openmetrics_data_model::gauge_value::Value::IntValue(v);
        Ok(())
    }

    pub fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        *self.value = openmetrics_data_model::gauge_value::Value::DoubleValue(v);
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct ExemplarValueEncoder<'a> {
    value: &'a mut f64,
}

impl<'a> ExemplarValueEncoder<'a> {
    pub fn encode(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        *self.value = v;
        Ok(())
    }
}

impl<K: ToString, V: ToString> From<&(K, V)> for openmetrics_data_model::Label {
    fn from(kv: &(K, V)) -> Self {
        openmetrics_data_model::Label {
            name: kv.0.to_string(),
            value: kv.1.to_string(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct CounterValueEncoder<'a> {
    value: &'a mut openmetrics_data_model::counter_value::Total,
}

impl<'a> CounterValueEncoder<'a> {
    pub fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        *self.value = openmetrics_data_model::counter_value::Total::DoubleValue(v);
        Ok(())
    }

    pub fn encode_u64(&mut self, v: u64) -> Result<(), std::fmt::Error> {
        *self.value = openmetrics_data_model::counter_value::Total::IntValue(v);
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct LabelSetEncoder<'a> {
    labels: &'a mut Vec<openmetrics_data_model::Label>,
}

impl<'a> LabelSetEncoder<'a> {
    pub fn encode_label(&mut self) -> LabelEncoder {
        LabelEncoder {
            labels: self.labels,
        }
    }
}

#[derive(Debug)]
pub(crate) struct LabelEncoder<'a> {
    labels: &'a mut Vec<openmetrics_data_model::Label>,
}

impl<'a> LabelEncoder<'a> {
    pub fn encode_label_key(&mut self) -> Result<LabelKeyEncoder, std::fmt::Error> {
        self.labels.push(openmetrics_data_model::Label::default());

        Ok(LabelKeyEncoder {
            label: self.labels.last_mut().expect("To find pushed label."),
        })
    }
}

#[derive(Debug)]
pub(crate) struct LabelKeyEncoder<'a> {
    label: &'a mut openmetrics_data_model::Label,
}

impl<'a> std::fmt::Write for LabelKeyEncoder<'a> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.label.name.write_str(s)
    }
}

impl<'a> LabelKeyEncoder<'a> {
    pub fn encode_label_value(self) -> Result<LabelValueEncoder<'a>, std::fmt::Error> {
        Ok(LabelValueEncoder {
            label_value: &mut self.label.value,
        })
    }
}

#[derive(Debug)]
pub(crate) struct LabelValueEncoder<'a> {
    label_value: &'a mut String,
}

impl<'a> LabelValueEncoder<'a> {
    pub fn finish(self) -> Result<(), std::fmt::Error> {
        Ok(())
    }
}

impl<'a> std::fmt::Write for LabelValueEncoder<'a> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.label_value.write_str(s)
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
    use std::collections::HashSet;
    use std::sync::atomic::AtomicI64;

    #[test]
    fn encode_counter_int() {
        let counter: Counter = Counter::default();
        let mut registry = Registry::default();
        registry.register("my_counter", "My counter", counter.clone());
        counter.inc();

        let metric_set = encode(&registry).unwrap();

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_counter", family.name);
        assert_eq!("My counter.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Counter as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(&metric_set) {
            openmetrics_data_model::metric_point::Value::CounterValue(value) => {
                let expected = openmetrics_data_model::counter_value::Total::IntValue(1);
                assert_eq!(Some(expected), value.total);
                assert_eq!(None, value.exemplar);
                assert_eq!(None, value.created);
            }
            _ => panic!("wrong value type"),
        }
    }

    #[test]
    fn encode_counter_double() {
        // Using `f64`
        let counter: Counter<f64> = Counter::default();
        let mut registry = Registry::default();
        registry.register("my_counter", "My counter", counter.clone());
        counter.inc();

        let metric_set = encode(&registry).unwrap();

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_counter", family.name);
        assert_eq!("My counter.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Counter as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(&metric_set) {
            openmetrics_data_model::metric_point::Value::CounterValue(value) => {
                // The counter should be encoded  as `DoubleValue`
                let expected = openmetrics_data_model::counter_value::Total::DoubleValue(1.0);
                assert_eq!(Some(expected), value.total);
                assert_eq!(None, value.exemplar);
                assert_eq!(None, value.created);
            }
            _ => panic!("wrong value type"),
        }
    }

    #[test]
    fn encode_counter_with_unit() {
        let mut registry = Registry::default();
        let counter: Counter = Counter::default();
        registry.register_with_unit("my_counter", "My counter", Unit::Seconds, counter);

        let metric_set = encode(&registry).unwrap();

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

        let metric_set = encode(&registry).unwrap();

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_counter_with_exemplar", family.name);
        assert_eq!("My counter with exemplar.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Counter as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(&metric_set) {
            openmetrics_data_model::metric_point::Value::CounterValue(value) => {
                // The counter should be encoded  as `DoubleValue`
                let expected = openmetrics_data_model::counter_value::Total::DoubleValue(1.0);
                assert_eq!(Some(expected), value.total);

                let exemplar = value.exemplar.as_ref().unwrap();
                assert_eq!(1.0, exemplar.value);

                let expected_label = {
                    openmetrics_data_model::Label {
                        name: "user_id".to_string(),
                        value: "42.0".to_string(),
                    }
                };
                assert_eq!(vec![expected_label], exemplar.label);
            }
            _ => panic!("wrong value type"),
        }
    }

    #[test]
    fn encode_gauge() {
        let mut registry = Registry::default();
        let gauge = Gauge::<i64, AtomicI64>::default();
        registry.register("my_gauge", "My gauge", gauge.clone());
        gauge.inc();

        let metric_set = encode(&registry).unwrap();
        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_gauge", family.name);
        assert_eq!("My gauge.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Gauge as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(&metric_set) {
            openmetrics_data_model::metric_point::Value::GaugeValue(value) => {
                let expected = openmetrics_data_model::gauge_value::Value::IntValue(1);
                assert_eq!(Some(expected), value.value);
            }
            _ => panic!("wrong value type"),
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

        family
            .get_or_create(&vec![
                ("method".to_string(), "POST".to_string()),
                ("status".to_string(), "200".to_string()),
            ])
            .inc();

        let metric_set = encode(&registry).unwrap();

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_counter_family", family.name);
        assert_eq!("My counter family.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Counter as i32,
            extract_metric_type(&metric_set)
        );

        // The order of the labels is not deterministic so we are testing the
        // value to be either
        let mut potential_method_value = HashSet::new();
        potential_method_value.insert("GET");
        potential_method_value.insert("POST");

        // the first metric
        let metric = family.metrics.first().unwrap();
        assert_eq!(2, metric.labels.len());
        assert_eq!("method", metric.labels[0].name);
        assert!(potential_method_value.remove(&metric.labels[0].value.as_str()));
        assert_eq!("status", metric.labels[1].name);
        assert_eq!("200", metric.labels[1].value);

        match extract_metric_point_value(&metric_set) {
            openmetrics_data_model::metric_point::Value::CounterValue(value) => {
                let expected = openmetrics_data_model::counter_value::Total::IntValue(1);
                assert_eq!(Some(expected), value.total);
                assert_eq!(None, value.exemplar);
                assert_eq!(None, value.created);
            }
            _ => panic!("wrong value type"),
        }

        // the second metric
        let metric2 = &family.metrics[1];
        assert_eq!(2, metric2.labels.len());
        assert_eq!("method", metric2.labels[0].name);
        assert!(potential_method_value.remove(&metric2.labels[0].value.as_str()));
        assert_eq!("status", metric2.labels[1].name);
        assert_eq!("200", metric2.labels[1].value);
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

        let metric_set = encode(&registry).unwrap();

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

        match extract_metric_point_value(&metric_set) {
            openmetrics_data_model::metric_point::Value::CounterValue(value) => {
                let expected = openmetrics_data_model::counter_value::Total::IntValue(1);
                assert_eq!(Some(expected), value.total);
                assert_eq!(None, value.exemplar);
                assert_eq!(None, value.created);
            }
            _ => panic!("wrong value type"),
        }
    }

    #[test]
    fn encode_histogram() {
        let mut registry = Registry::default();
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);

        let metric_set = encode(&registry).unwrap();

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_histogram", family.name);
        assert_eq!("My histogram.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Histogram as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(&metric_set) {
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
            _ => panic!("wrong value type"),
        }
    }

    #[test]
    fn encode_histogram_with_exemplars() {
        let mut registry = Registry::default();
        let histogram = HistogramWithExemplars::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0, Some(vec![("user_id".to_string(), 42u64)]));

        let metric_set = encode(&registry).unwrap();

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_histogram", family.name);
        assert_eq!("My histogram.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Histogram as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(&metric_set) {
            openmetrics_data_model::metric_point::Value::HistogramValue(value) => {
                let exemplar = value.buckets.first().unwrap().exemplar.as_ref().unwrap();
                assert_eq!(1.0, exemplar.value);

                let expected_label = {
                    openmetrics_data_model::Label {
                        name: "user_id".to_string(),
                        value: "42".to_string(),
                    }
                };
                assert_eq!(vec![expected_label], exemplar.label);
            }
            _ => panic!("wrong value type"),
        }
    }

    #[test]
    fn encode_family_counter_histogram() {
        let mut registry = Registry::default();

        let counter_family = Family::<Vec<(String, String)>, Counter>::default();
        let histogram_family =
            Family::<Vec<(String, String)>, Histogram>::new_with_constructor(|| {
                Histogram::new(exponential_buckets(1.0, 2.0, 10))
            });

        registry.register("my_counter", "My counter", counter_family.clone());
        registry.register("my_histogram", "My histogram", histogram_family.clone());

        counter_family
            .get_or_create(&vec![("path".to_string(), "/".to_string())])
            .inc();

        histogram_family
            .get_or_create(&vec![("path".to_string(), "/".to_string())])
            .observe(1.0);

        let metric_set = encode(&registry).unwrap();
        assert_eq!("my_counter", metric_set.metric_families[0].name);
        assert_eq!("my_histogram", metric_set.metric_families[1].name);
    }

    #[test]
    fn encode_family_and_counter_and_histogram() {
        let mut registry = Registry::default();

        // Family
        let counter_family = Family::<Vec<(String, String)>, Counter>::default();
        let histogram_family =
            Family::<Vec<(String, String)>, Histogram>::new_with_constructor(|| {
                Histogram::new(exponential_buckets(1.0, 2.0, 10))
            });

        registry.register("my_family_counter", "My counter", counter_family.clone());
        registry.register(
            "my_family_histogram",
            "My histogram",
            histogram_family.clone(),
        );

        counter_family
            .get_or_create(&vec![("path".to_string(), "/".to_string())])
            .inc();

        histogram_family
            .get_or_create(&vec![("path".to_string(), "/".to_string())])
            .observe(1.0);

        // Counter
        let counter: Counter = Counter::default();
        registry.register("my_counter", "My counter", counter.clone());
        counter.inc();

        // Histogram
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);

        let metric_set = encode(&registry).unwrap();
        assert_eq!("my_family_counter", metric_set.metric_families[0].name);
        assert_eq!("my_family_histogram", metric_set.metric_families[1].name);
    }

    #[test]
    fn encode_info() {
        let mut registry = Registry::default();
        let info = Info::new(vec![("os".to_string(), "GNU/linux".to_string())]);
        registry.register("my_info_metric", "My info metric", info);

        let metric_set = encode(&registry).unwrap();

        let family = metric_set.metric_families.first().unwrap();
        assert_eq!("my_info_metric", family.name);
        assert_eq!("My info metric.", family.help);

        assert_eq!(
            openmetrics_data_model::MetricType::Info as i32,
            extract_metric_type(&metric_set)
        );

        match extract_metric_point_value(&metric_set) {
            openmetrics_data_model::metric_point::Value::InfoValue(value) => {
                assert_eq!(1, value.info.len());

                let info = value.info.first().unwrap();
                assert_eq!("os", info.name);
                assert_eq!("GNU/linux", info.value);
            }
            _ => panic!("wrong value type"),
        }
    }

    fn extract_metric_type(metric_set: &openmetrics_data_model::MetricSet) -> i32 {
        let family = metric_set.metric_families.first().unwrap();
        family.r#type
    }

    fn extract_metric_point_value(
        metric_set: &openmetrics_data_model::MetricSet,
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
