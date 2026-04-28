//! Prometheus protobuf implementation.
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
//! // Returns `Vec<MetricFamily>`, the top-level container type. Please refer to [metrics.proto](https://github.com/prometheus/prometheus/blob/main/prompb/io/prometheus/client/metrics.proto) for details.
//! let metric_families = encode(&registry).unwrap();
//!
//! let family = metric_families.first().unwrap();
//! assert_eq!("my_counter_total", family.name);
//! assert_eq!("This is my counter.", family.help);
//! ```
//!
//! For wire-format exposition, serialize each returned `MetricFamily` with
//! length-delimited protobuf framing. [`encode_to_vec`] provides the exact
//! payload used with
//! `application/vnd.google.protobuf;proto=io.prometheus.client.MetricFamily;encoding=delimited`.

// Allowing some lints here as the `io.prometheus.client.rs` file is generated.
#[allow(missing_docs, clippy::derive_partial_eq_without_eq)]
/// Data models generated from Prometheus `io.prometheus.client` protobuf.
pub mod prometheus_data_model {
    include!(concat!(env!("OUT_DIR"), "/io.prometheus.client.rs"));
}

use prost::Message;
use std::{borrow::Cow, collections::HashMap};

use crate::metrics::MetricType;
use crate::registry::{Registry, Unit};
use crate::{metrics::exemplar::Exemplar, registry::Prefix};

use super::{EncodeCounterValue, EncodeExemplarValue, EncodeGaugeValue, EncodeLabelSet};

/// Encode the metrics registered with the provided [`Registry`] into
/// Prometheus `MetricFamily` messages.
pub fn encode(
    registry: &Registry,
) -> Result<Vec<prometheus_data_model::MetricFamily>, std::fmt::Error> {
    let mut metric_families = Vec::new();
    let mut descriptor_encoder = DescriptorEncoder::new(&mut metric_families).into();
    registry.encode(&mut descriptor_encoder)?;
    Ok(metric_families)
}

/// Encode the metrics registered with the provided [`Registry`] into a
/// length-delimited Prometheus protobuf payload.
pub fn encode_to_vec(registry: &Registry) -> Result<Vec<u8>, EncodeError> {
    let metric_families = encode(registry)?;
    let mut encoded = Vec::new();

    for metric_family in metric_families {
        metric_family.encode_length_delimited(&mut encoded)?;
    }

    Ok(encoded)
}

/// Errors returned by [`encode_to_vec`].
#[derive(Debug)]
pub enum EncodeError {
    /// A metric failed to encode into the intermediate protobuf data model.
    Fmt(std::fmt::Error),
    /// The generated protobuf message failed to serialize.
    Protobuf(prost::EncodeError),
}

impl std::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EncodeError::Fmt(_) => f.write_str("failed to encode metrics into Prometheus protobuf"),
            EncodeError::Protobuf(err) => err.fmt(f),
        }
    }
}

impl std::error::Error for EncodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EncodeError::Fmt(err) => Some(err),
            EncodeError::Protobuf(err) => Some(err),
        }
    }
}

impl From<std::fmt::Error> for EncodeError {
    fn from(err: std::fmt::Error) -> Self {
        EncodeError::Fmt(err)
    }
}

impl From<prost::EncodeError> for EncodeError {
    fn from(err: prost::EncodeError) -> Self {
        EncodeError::Protobuf(err)
    }
}

impl From<MetricType> for prometheus_data_model::MetricType {
    fn from(metric_type: MetricType) -> Self {
        match metric_type {
            MetricType::Counter => prometheus_data_model::MetricType::Counter,
            MetricType::Gauge => prometheus_data_model::MetricType::Gauge,
            MetricType::Histogram => prometheus_data_model::MetricType::Histogram,
            // Prometheus does not have a dedicated info type; expose it as the
            // conventional `<name>_info` gauge with value `1`.
            MetricType::Info => prometheus_data_model::MetricType::Gauge,
            MetricType::Unknown => prometheus_data_model::MetricType::Untyped,
        }
    }
}

fn metric_family_name(
    prefix: Option<&Prefix>,
    name: &str,
    unit: Option<&Unit>,
    metric_type: MetricType,
) -> String {
    let mut full_name = String::new();

    if let Some(prefix) = prefix {
        full_name.push_str(prefix.as_str());
        full_name.push('_');
    }

    full_name.push_str(name);

    if let Some(unit) = unit {
        full_name.push('_');
        full_name.push_str(unit.as_str());
    }

    match metric_type {
        MetricType::Counter => full_name.push_str("_total"),
        MetricType::Info => full_name.push_str("_info"),
        MetricType::Gauge | MetricType::Histogram | MetricType::Unknown => {}
    }

    full_name
}

/// Metric Descriptor encoder for protobuf encoding.
#[derive(Debug)]
pub(crate) struct DescriptorEncoder<'a> {
    metric_families: &'a mut Vec<prometheus_data_model::MetricFamily>,
    prefix: Option<&'a Prefix>,
    labels: &'a [(Cow<'static, str>, Cow<'static, str>)],
}

impl DescriptorEncoder<'_> {
    pub(crate) fn new(
        metric_families: &mut Vec<prometheus_data_model::MetricFamily>,
    ) -> DescriptorEncoder<'_> {
        DescriptorEncoder {
            metric_families,
            prefix: Default::default(),
            labels: Default::default(),
        }
    }

    pub(crate) fn with_prefix_and_labels<'s>(
        &'s mut self,
        prefix: Option<&'s Prefix>,
        labels: &'s [(Cow<'static, str>, Cow<'static, str>)],
    ) -> DescriptorEncoder<'s> {
        DescriptorEncoder {
            prefix,
            labels,
            metric_families: self.metric_families,
        }
    }

    pub fn encode_descriptor<'s>(
        &'s mut self,
        name: &str,
        help: &str,
        unit: Option<&Unit>,
        metric_type: MetricType,
    ) -> Result<MetricEncoder<'s>, std::fmt::Error> {
        let family = prometheus_data_model::MetricFamily {
            name: metric_family_name(self.prefix, name, unit, metric_type),
            r#type: prometheus_data_model::MetricType::from(metric_type) as i32,
            unit: unit
                .map(|unit| unit.as_str().to_string())
                .unwrap_or_default(),
            help: help.to_string(),
            ..Default::default()
        };
        let mut labels = vec![];
        self.labels.encode(
            &mut LabelSetEncoder {
                labels: &mut labels,
            }
            .into(),
        )?;
        self.metric_families.push(family);

        Ok(MetricEncoder {
            family: &mut self
                .metric_families
                .last_mut()
                .expect("previous push")
                .metric,
            metric_type,
            labels,
        })
    }
}

/// Encoder for protobuf encoding.
///
/// This is an inner type for [`super::MetricEncoder`].
#[derive(Debug)]
pub(crate) struct MetricEncoder<'f> {
    /// OpenMetrics metric type of the metric.
    metric_type: MetricType,
    /// Vector of OpenMetrics metrics to which encoded metrics are added.
    family: &'f mut Vec<prometheus_data_model::Metric>,
    /// Labels to be added to each metric.
    labels: Vec<prometheus_data_model::LabelPair>,
}

impl MetricEncoder<'_> {
    pub fn encode_counter<
        S: EncodeLabelSet,
        CounterValue: EncodeCounterValue,
        ExemplarValue: EncodeExemplarValue,
    >(
        &mut self,
        v: &CounterValue,
        exemplar: Option<&Exemplar<S, ExemplarValue>>,
    ) -> Result<(), std::fmt::Error> {
        let mut value = 0.0;
        let mut e = CounterValueEncoder { value: &mut value }.into();
        v.encode(&mut e)?;

        self.family.push(prometheus_data_model::Metric {
            label: self.labels.clone(),
            counter: Some(prometheus_data_model::Counter {
                value,
                exemplar: exemplar.map(TryInto::try_into).transpose()?,
                start_timestamp: None,
            }),
            ..Default::default()
        });

        Ok(())
    }

    pub fn encode_gauge<GaugeValue: EncodeGaugeValue>(
        &mut self,
        v: &GaugeValue,
    ) -> Result<(), std::fmt::Error> {
        let mut value = 0.0;
        let mut e = GaugeValueEncoder { value: &mut value }.into();
        v.encode(&mut e)?;

        self.family.push(prometheus_data_model::Metric {
            label: self.labels.clone(),
            gauge: Some(prometheus_data_model::Gauge { value }),
            ..Default::default()
        });

        Ok(())
    }

    pub fn encode_info(
        &mut self,
        label_set: &impl super::EncodeLabelSet,
    ) -> Result<(), std::fmt::Error> {
        let mut labels = self.labels.clone();
        label_set.encode(
            &mut LabelSetEncoder {
                labels: &mut labels,
            }
            .into(),
        )?;

        self.family.push(prometheus_data_model::Metric {
            label: labels,
            gauge: Some(prometheus_data_model::Gauge { value: 1.0 }),
            ..Default::default()
        });

        Ok(())
    }

    pub fn encode_family<S: EncodeLabelSet>(
        &mut self,
        label_set: &S,
    ) -> Result<MetricEncoder<'_>, std::fmt::Error> {
        let mut labels = self.labels.clone();
        label_set.encode(
            &mut LabelSetEncoder {
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
        let mut cumulative_count = 0;
        let bucket = buckets
            .iter()
            .enumerate()
            .map(|(i, (upper_bound, count))| {
                cumulative_count += count;
                Ok(prometheus_data_model::Bucket {
                    cumulative_count,
                    // not needed; if set would override cumulative_count.
                    cumulative_count_float: 0.0,
                    upper_bound: *upper_bound,
                    exemplar: exemplars
                        .and_then(|exemplars| exemplars.get(&i).map(|exemplar| exemplar.try_into()))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, std::fmt::Error>>()?;

        self.family.push(prometheus_data_model::Metric {
            label: self.labels.clone(),
            histogram: Some(prometheus_data_model::Histogram {
                sample_count: count,
                sample_count_float: 0.0,
                sample_sum: sum,
                bucket,
                start_timestamp: None,
                ..Default::default()
            }),
            ..Default::default()
        });

        Ok(())
    }
}

impl<S: EncodeLabelSet, V: EncodeExemplarValue> TryFrom<&Exemplar<S, V>>
    for prometheus_data_model::Exemplar
{
    type Error = std::fmt::Error;

    fn try_from(exemplar: &Exemplar<S, V>) -> Result<Self, Self::Error> {
        let mut value = f64::default();
        exemplar
            .value
            .encode(ExemplarValueEncoder { value: &mut value }.into())?;

        let mut label = vec![];
        exemplar
            .label_set
            .encode(&mut LabelSetEncoder { labels: &mut label }.into())?;

        Ok(prometheus_data_model::Exemplar {
            label,
            value,
            timestamp: exemplar.timestamp.map(Into::into),
        })
    }
}

#[derive(Debug)]
pub(crate) struct GaugeValueEncoder<'a> {
    value: &'a mut f64,
}

impl GaugeValueEncoder<'_> {
    pub fn encode_u32(&mut self, v: u32) -> Result<(), std::fmt::Error> {
        self.encode_f64(f64::from(v))
    }

    pub fn encode_u64(&mut self, v: u64) -> Result<(), std::fmt::Error> {
        self.encode_f64(v as f64)
    }

    pub fn encode_i64(&mut self, v: i64) -> Result<(), std::fmt::Error> {
        self.encode_f64(v as f64)
    }

    pub fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        *self.value = v;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct ExemplarValueEncoder<'a> {
    value: &'a mut f64,
}

impl ExemplarValueEncoder<'_> {
    pub fn encode(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        *self.value = v;
        Ok(())
    }
}

impl<K: ToString, V: ToString> From<&(K, V)> for prometheus_data_model::LabelPair {
    fn from(kv: &(K, V)) -> Self {
        prometheus_data_model::LabelPair {
            name: kv.0.to_string(),
            value: kv.1.to_string(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct CounterValueEncoder<'a> {
    value: &'a mut f64,
}

impl CounterValueEncoder<'_> {
    pub fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        *self.value = v;
        Ok(())
    }

    pub fn encode_u64(&mut self, v: u64) -> Result<(), std::fmt::Error> {
        *self.value = v as f64;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct LabelSetEncoder<'a> {
    labels: &'a mut Vec<prometheus_data_model::LabelPair>,
}

impl LabelSetEncoder<'_> {
    pub fn encode_label(&mut self) -> LabelEncoder<'_> {
        LabelEncoder {
            labels: self.labels,
        }
    }
}

#[derive(Debug)]
pub(crate) struct LabelEncoder<'a> {
    labels: &'a mut Vec<prometheus_data_model::LabelPair>,
}

impl LabelEncoder<'_> {
    pub fn encode_label_key(&mut self) -> Result<LabelKeyEncoder<'_>, std::fmt::Error> {
        self.labels
            .push(prometheus_data_model::LabelPair::default());

        Ok(LabelKeyEncoder {
            label: self.labels.last_mut().expect("To find pushed label."),
        })
    }
}

#[derive(Debug)]
pub(crate) struct LabelKeyEncoder<'a> {
    label: &'a mut prometheus_data_model::LabelPair,
}

impl std::fmt::Write for LabelKeyEncoder<'_> {
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

impl LabelValueEncoder<'_> {
    pub fn finish(self) -> Result<(), std::fmt::Error> {
        Ok(())
    }
}

impl std::fmt::Write for LabelValueEncoder<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.label_value.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use prost_types::Timestamp;

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
    use std::sync::atomic::AtomicU64;
    use std::time::SystemTime;

    #[test]
    fn encode_counter_int() {
        let counter: Counter = Counter::default();
        let mut registry = Registry::default();
        registry.register("my_counter", "My counter", counter.clone());
        counter.inc();

        let metric_families = encode(&registry).unwrap();
        let family = metric_families.first().unwrap();
        assert_eq!("my_counter_total", family.name);
        assert_eq!("My counter.", family.help);
        assert_eq!(
            prometheus_data_model::MetricType::Counter as i32,
            family.r#type
        );

        let metric = family.metric.first().unwrap();
        assert_eq!(1.0, metric.counter.as_ref().unwrap().value);
    }

    #[test]
    fn encode_counter_with_unit() {
        let counter: Counter = Counter::default();
        let mut registry = Registry::default();
        registry.register_with_unit("my_counter", "My counter", Unit::Seconds, counter);

        let metric_families = encode(&registry).unwrap();
        let family = metric_families.first().unwrap();
        assert_eq!("my_counter_seconds_total", family.name);
        assert_eq!("seconds", family.unit);
    }

    #[test]
    fn encode_counter_with_exemplar() {
        let now = SystemTime::now();
        let now_ts: Timestamp = now.into();

        let mut registry = Registry::default();
        let counter: CounterWithExemplar<Vec<(String, f64)>, f64> = CounterWithExemplar::default();
        registry.register("my_counter", "My counter", counter.clone());

        counter.inc_by(1.0, Some(vec![("user_id".to_string(), 42.0)]), None);

        let metric_families = encode(&registry).unwrap();
        let exemplar = metric_families[0].metric[0]
            .counter
            .as_ref()
            .unwrap()
            .exemplar
            .as_ref()
            .unwrap();
        assert_eq!(1.0, exemplar.value);
        assert_eq!(None, exemplar.timestamp);
        assert_eq!("user_id", exemplar.label[0].name);
        assert_eq!("42.0", exemplar.label[0].value);

        counter.inc_by(1.0, Some(vec![("user_id".to_string(), 99.0)]), Some(now));

        let metric_families = encode(&registry).unwrap();
        let counter = metric_families[0].metric[0].counter.as_ref().unwrap();
        assert_eq!(2.0, counter.value);
        let exemplar = counter.exemplar.as_ref().unwrap();
        assert_eq!(1.0, exemplar.value);
        assert_eq!(Some(now_ts), exemplar.timestamp.clone());
        assert_eq!("99.0", exemplar.label[0].value);
    }

    #[test]
    fn encode_gauge() {
        let gauge = Gauge::<i64, AtomicI64>::default();
        let mut registry = Registry::default();
        registry.register("my_gauge", "My gauge", gauge.clone());
        gauge.inc();

        let metric_families = encode(&registry).unwrap();
        let family = metric_families.first().unwrap();
        assert_eq!("my_gauge", family.name.as_str());
        assert_eq!(
            prometheus_data_model::MetricType::Gauge as i32,
            family.r#type
        );
        assert_eq!(1.0, family.metric[0].gauge.as_ref().unwrap().value);
    }

    #[test]
    fn encode_gauge_u64_max() {
        let gauge = Gauge::<u64, AtomicU64>::default();
        let mut registry = Registry::default();
        registry.register("my_gauge", "My gauge", gauge.clone());
        gauge.set(u64::MAX);

        let metric_families = encode(&registry).unwrap();
        assert_eq!(
            u64::MAX as f64,
            metric_families[0].metric[0].gauge.as_ref().unwrap().value
        );
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

        let metric_families = encode(&registry).unwrap();
        let family = metric_families.first().unwrap();
        assert_eq!("my_counter_family_total", family.name.as_str());
        assert_eq!(2, family.metric.len());

        let mut potential_method_value = HashSet::new();
        potential_method_value.insert("GET");
        potential_method_value.insert("POST");

        let metric = family.metric.first().unwrap();
        assert_eq!(2, metric.label.len());
        assert_eq!("method", metric.label[0].name);
        assert!(potential_method_value.remove(metric.label[0].value.as_str()));
        assert_eq!("status", metric.label[1].name);
        assert_eq!("200", metric.label[1].value);

        let metric2 = &family.metric[1];
        assert_eq!(2, metric2.label.len());
        assert_eq!("method", metric2.label[0].name);
        assert!(potential_method_value.remove(metric2.label[0].value.as_str()));
        assert_eq!("status", metric2.label[1].name);
        assert_eq!("200", metric2.label[1].value);
    }

    #[test]
    fn encode_counter_family_with_prefix_and_label() {
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

        let metric_families = encode(&registry).unwrap();
        let family = metric_families.first().unwrap();
        assert_eq!("my_prefix_my_counter_family_total", family.name.as_str());

        let metric = family.metric.first().unwrap();
        assert_eq!("my_key", metric.label[0].name);
        assert_eq!("my_value", metric.label[0].value);
        assert_eq!("method", metric.label[1].name);
        assert_eq!("GET", metric.label[1].value);
        assert_eq!("status", metric.label[2].name);
        assert_eq!("200", metric.label[2].value);
    }

    #[test]
    fn encode_histogram() {
        let mut registry = Registry::default();
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);

        let metric_families = encode(&registry).unwrap();
        let family = metric_families.first().unwrap();
        assert_eq!("my_histogram", family.name.as_str());
        assert_eq!(
            prometheus_data_model::MetricType::Histogram as i32,
            family.r#type
        );

        let histogram = family.metric[0].histogram.as_ref().unwrap();
        assert_eq!(1, histogram.sample_count);
        assert_eq!(1.0, histogram.sample_sum);
        assert_eq!(11, histogram.bucket.len());
        assert_eq!(1, histogram.bucket[0].cumulative_count);
        assert_eq!(1.0, histogram.bucket[0].upper_bound);
        assert_eq!(f64::MAX, histogram.bucket.last().unwrap().upper_bound);
    }

    #[test]
    fn encode_histogram_with_exemplars() {
        let now = SystemTime::now();
        let now_ts: Timestamp = now.into();

        let histogram = HistogramWithExemplars::new(exponential_buckets(1.0, 2.0, 10));
        let mut registry = Registry::default();
        registry.register("my_histogram", "My histogram", histogram.clone());

        histogram.observe(1.0, Some(vec![("user_id".to_string(), 42u64)]), None);

        let metric_families = encode(&registry).unwrap();
        let exemplar = metric_families[0].metric[0]
            .histogram
            .as_ref()
            .unwrap()
            .bucket[0]
            .exemplar
            .as_ref()
            .unwrap();
        assert_eq!(1.0, exemplar.value);
        assert_eq!(None, exemplar.timestamp);
        assert_eq!("42", exemplar.label[0].value);

        histogram.observe(2.0, Some(vec![("user_id".to_string(), 99u64)]), Some(now));

        let metric_families = encode(&registry).unwrap();
        let exemplar = metric_families[0].metric[0]
            .histogram
            .as_ref()
            .unwrap()
            .bucket[1]
            .exemplar
            .as_ref()
            .unwrap();
        assert_eq!(2.0, exemplar.value);
        assert_eq!(Some(now_ts), exemplar.timestamp.clone());
        assert_eq!("99", exemplar.label[0].value);
    }

    #[test]
    fn encode_family_and_counter_and_histogram() {
        let mut registry = Registry::default();

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

        let counter: Counter = Counter::default();
        registry.register("my_counter", "My counter", counter.clone());
        counter.inc();

        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        registry.register("my_histogram", "My histogram", histogram.clone());
        histogram.observe(1.0);

        let metric_families = encode(&registry).unwrap();
        assert_eq!("my_family_counter_total", metric_families[0].name);
        assert_eq!("my_family_histogram", metric_families[1].name);
        assert_eq!("my_counter_total", metric_families[2].name);
        assert_eq!("my_histogram", metric_families[3].name);
    }

    #[test]
    fn encode_info() {
        let info = Info::new(vec![("os".to_string(), "GNU/linux".to_string())]);
        let mut registry = Registry::default();
        registry.register("my_info_metric", "My info metric", info);

        let metric_families = encode(&registry).unwrap();
        let family = metric_families.first().unwrap();
        assert_eq!("my_info_metric_info", family.name.as_str());
        assert_eq!(
            prometheus_data_model::MetricType::Gauge as i32,
            family.r#type
        );

        let metric = family.metric.first().unwrap();
        assert_eq!(1.0, metric.gauge.as_ref().unwrap().value);
        assert_eq!("os", metric.label[0].name);
        assert_eq!("GNU/linux", metric.label[0].value);
    }

    #[test]
    fn encode_to_vec_length_delimited() {
        let counter: Counter = Counter::default();
        let mut registry = Registry::default();
        registry.register("my_counter", "My counter", counter.clone());
        counter.inc();

        let payload = encode_to_vec(&registry).unwrap();
        let family =
            prometheus_data_model::MetricFamily::decode_length_delimited(payload.as_slice())
                .unwrap();

        assert_eq!("my_counter_total", family.name);
        assert_eq!(1.0, family.metric[0].counter.as_ref().unwrap().value);
    }
}
