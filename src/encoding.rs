//! Exposition format implementations.

pub use prometheus_client_derive_encode::*;

use crate::metrics::exemplar::Exemplar;
use crate::metrics::MetricType;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write;
use std::ops::Deref;

#[cfg(feature = "protobuf")]
pub mod protobuf;
pub mod text;

/// Trait implemented by each metric type, e.g.
/// [`Counter`](crate::metrics::counter::Counter), to implement its encoding in
/// the OpenMetric text format.
pub trait EncodeMetric {
    /// Encode the given instance in the OpenMetrics text encoding.
    fn encode(&self, encoder: MetricEncoder<'_, '_>) -> Result<(), std::fmt::Error>;

    /// The OpenMetrics metric type of the instance.
    // One can not use [`TypedMetric`] directly, as associated constants are not
    // object safe and thus can not be used with dynamic dispatching.
    fn metric_type(&self) -> MetricType;
}

impl EncodeMetric for Box<dyn EncodeMetric> {
    fn encode(&self, encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        self.deref().encode(encoder)
    }

    fn metric_type(&self) -> MetricType {
        self.deref().metric_type()
    }
}

/// Encoder for a metric.
///
// `MetricEncoder` does not take a trait parameter for `writer` and `labels`
// because `EncodeMetric` which uses `MetricEncoder` needs to be usable as a
// trait object in order to be able to register different metric types with a
// `Registry`. Trait objects can not use type parameters.
//
// TODO: Alternative solutions to the above are very much appreciated.
#[derive(Debug)]
pub struct MetricEncoder<'a, 'b>(MetricEncoderInner<'a, 'b>);

#[derive(Debug)]
enum MetricEncoderInner<'a, 'b> {
    Text(text::MetricEncoder<'a, 'b>),

    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::MetricEncoder<'a>),
}

impl<'a, 'b> From<text::MetricEncoder<'a, 'b>> for MetricEncoder<'a, 'b> {
    fn from(e: text::MetricEncoder<'a, 'b>) -> Self {
        Self(MetricEncoderInner::Text(e))
    }
}

#[cfg(feature = "protobuf")]
impl<'a, 'b> From<protobuf::MetricEncoder<'a>> for MetricEncoder<'a, 'b> {
    fn from(e: protobuf::MetricEncoder<'a>) -> Self {
        Self(MetricEncoderInner::Protobuf(e))
    }
}

macro_rules! for_both_mut {
    ($self:expr, $inner:ident, $pattern:pat, $fn:expr) => {
        match &mut $self.0 {
            $inner::Text($pattern) => $fn,
            #[cfg(feature = "protobuf")]
            $inner::Protobuf($pattern) => $fn,
        }
    };
}

macro_rules! for_both {
    ($self:expr, $inner:ident, $pattern:pat, $fn:expr) => {
        match $self.0 {
            $inner::Text($pattern) => $fn,
            #[cfg(feature = "protobuf")]
            $inner::Protobuf($pattern) => $fn,
        }
    };
}

impl<'a, 'b> MetricEncoder<'a, 'b> {
    /// Encode a counter.
    pub fn encode_counter<
        S: EncodeLabelSet,
        CounterValue: EncodeCounterValue,
        ExemplarValue: EncodeExemplarValue,
    >(
        &mut self,
        v: &CounterValue,
        exemplar: Option<&Exemplar<S, ExemplarValue>>,
    ) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, MetricEncoderInner, e, e.encode_counter(v, exemplar))
    }

    /// Encode a gauge.
    pub fn encode_gauge<GaugeValue: EncodeGaugeValue>(
        &mut self,
        v: &GaugeValue,
    ) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, MetricEncoderInner, e, e.encode_gauge(v))
    }

    /// Encode an info.
    pub fn encode_info(&mut self, label_set: &impl EncodeLabelSet) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, MetricEncoderInner, e, e.encode_info(label_set))
    }

    /// Encode a histogram.
    pub fn encode_histogram<S: EncodeLabelSet>(
        &mut self,
        sum: f64,
        count: u64,
        buckets: &[(f64, u64)],
        exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
    ) -> Result<(), std::fmt::Error> {
        for_both_mut!(
            self,
            MetricEncoderInner,
            e,
            e.encode_histogram(sum, count, buckets, exemplars)
        )
    }

    /// Encode a metric family.
    pub fn encode_family<'c, 'd, S: EncodeLabelSet>(
        &'c mut self,
        label_set: &'d S,
    ) -> Result<MetricEncoder<'c, 'd>, std::fmt::Error> {
        for_both_mut!(
            self,
            MetricEncoderInner,
            e,
            e.encode_family(label_set).map(Into::into)
        )
    }
}

/// An encodable label set.
pub trait EncodeLabelSet {
    /// Encode oneself into the given encoder.
    fn encode(&self, encoder: LabelSetEncoder) -> Result<(), std::fmt::Error>;
}

impl<'a> From<text::LabelSetEncoder<'a>> for LabelSetEncoder<'a> {
    fn from(e: text::LabelSetEncoder<'a>) -> Self {
        Self(LabelSetEncoderInner::Text(e))
    }
}

/// Encoder for a label set.
#[derive(Debug)]
pub struct LabelSetEncoder<'a>(LabelSetEncoderInner<'a>);

#[derive(Debug)]
enum LabelSetEncoderInner<'a> {
    Text(text::LabelSetEncoder<'a>),
    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::LabelSetEncoder<'a>),
}

#[cfg(feature = "protobuf")]
impl<'a> From<protobuf::LabelSetEncoder<'a>> for LabelSetEncoder<'a> {
    fn from(e: protobuf::LabelSetEncoder<'a>) -> Self {
        Self(LabelSetEncoderInner::Protobuf(e))
    }
}

impl<'a> LabelSetEncoder<'a> {
    /// Encode the given label.
    pub fn encode_label(&mut self) -> LabelEncoder {
        for_both_mut!(self, LabelSetEncoderInner, e, e.encode_label().into())
    }
}

/// An encodable label.
pub trait EncodeLabel {
    /// Encode oneself into the given encoder.
    fn encode(&self, encoder: LabelEncoder) -> Result<(), std::fmt::Error>;
}

/// Encoder for a label.
#[derive(Debug)]
pub struct LabelEncoder<'a>(LabelEncoderInner<'a>);

#[derive(Debug)]
enum LabelEncoderInner<'a> {
    Text(text::LabelEncoder<'a>),
    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::LabelEncoder<'a>),
}

impl<'a> From<text::LabelEncoder<'a>> for LabelEncoder<'a> {
    fn from(e: text::LabelEncoder<'a>) -> Self {
        Self(LabelEncoderInner::Text(e))
    }
}

#[cfg(feature = "protobuf")]
impl<'a> From<protobuf::LabelEncoder<'a>> for LabelEncoder<'a> {
    fn from(e: protobuf::LabelEncoder<'a>) -> Self {
        Self(LabelEncoderInner::Protobuf(e))
    }
}

impl<'a> LabelEncoder<'a> {
    /// Encode a label.
    pub fn encode_label_key(&mut self) -> Result<LabelKeyEncoder, std::fmt::Error> {
        for_both_mut!(
            self,
            LabelEncoderInner,
            e,
            e.encode_label_key().map(Into::into)
        )
    }
}

/// An encodable label key.
pub trait EncodeLabelKey {
    /// Encode oneself into the given encoder.
    fn encode(&self, encoder: &mut LabelKeyEncoder) -> Result<(), std::fmt::Error>;
}

/// Encoder for a label key.
#[derive(Debug)]
pub struct LabelKeyEncoder<'a>(LabelKeyEncoderInner<'a>);

#[derive(Debug)]
enum LabelKeyEncoderInner<'a> {
    Text(text::LabelKeyEncoder<'a>),
    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::LabelKeyEncoder<'a>),
}

impl<'a> From<text::LabelKeyEncoder<'a>> for LabelKeyEncoder<'a> {
    fn from(e: text::LabelKeyEncoder<'a>) -> Self {
        Self(LabelKeyEncoderInner::Text(e))
    }
}

#[cfg(feature = "protobuf")]
impl<'a> From<protobuf::LabelKeyEncoder<'a>> for LabelKeyEncoder<'a> {
    fn from(e: protobuf::LabelKeyEncoder<'a>) -> Self {
        Self(LabelKeyEncoderInner::Protobuf(e))
    }
}

impl<'a> std::fmt::Write for LabelKeyEncoder<'a> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for_both_mut!(self, LabelKeyEncoderInner, e, e.write_str(s))
    }
}

impl<'a> LabelKeyEncoder<'a> {
    /// Encode a label value.
    pub fn encode_label_value(self) -> Result<LabelValueEncoder<'a>, std::fmt::Error> {
        for_both!(
            self,
            LabelKeyEncoderInner,
            e,
            e.encode_label_value().map(LabelValueEncoder::from)
        )
    }
}
impl<T: EncodeLabel, const N: usize> EncodeLabelSet for [T; N] {
    fn encode(&self, encoder: LabelSetEncoder) -> Result<(), std::fmt::Error> {
        self.as_ref().encode(encoder)
    }
}

impl<T: EncodeLabel> EncodeLabelSet for &[T] {
    fn encode(&self, mut encoder: LabelSetEncoder) -> Result<(), std::fmt::Error> {
        if self.is_empty() {
            return Ok(());
        }

        for label in self.iter() {
            label.encode(encoder.encode_label())?
        }

        Ok(())
    }
}

impl<T: EncodeLabel> EncodeLabelSet for Vec<T> {
    fn encode(&self, encoder: LabelSetEncoder) -> Result<(), std::fmt::Error> {
        self.as_slice().encode(encoder)
    }
}

impl EncodeLabelSet for () {
    fn encode(&self, _encoder: LabelSetEncoder) -> Result<(), std::fmt::Error> {
        Ok(())
    }
}

impl<K: EncodeLabelKey, V: EncodeLabelValue> EncodeLabel for (K, V) {
    fn encode(&self, mut encoder: LabelEncoder) -> Result<(), std::fmt::Error> {
        let (key, value) = self;

        let mut label_key_encoder = encoder.encode_label_key()?;
        key.encode(&mut label_key_encoder)?;

        let mut label_value_encoder = label_key_encoder.encode_label_value()?;
        value.encode(&mut label_value_encoder)?;
        label_value_encoder.finish()?;

        Ok(())
    }
}

impl EncodeLabelKey for &str {
    fn encode(&self, encoder: &mut LabelKeyEncoder) -> Result<(), std::fmt::Error> {
        encoder.write_str(self)?;
        Ok(())
    }
}

impl EncodeLabelKey for String {
    fn encode(&self, encoder: &mut LabelKeyEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelKey::encode(&self.as_str(), encoder)
    }
}

impl<'a> EncodeLabelKey for Cow<'a, str> {
    fn encode(&self, encoder: &mut LabelKeyEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelKey::encode(&self.as_ref(), encoder)
    }
}

/// An encodable label value.
pub trait EncodeLabelValue {
    /// Encode oneself into the given encoder.
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error>;
}

/// Encoder for a label value.
#[derive(Debug)]
pub struct LabelValueEncoder<'a>(LabelValueEncoderInner<'a>);

impl<'a> From<text::LabelValueEncoder<'a>> for LabelValueEncoder<'a> {
    fn from(e: text::LabelValueEncoder<'a>) -> Self {
        LabelValueEncoder(LabelValueEncoderInner::Text(e))
    }
}

#[cfg(feature = "protobuf")]
impl<'a> From<protobuf::LabelValueEncoder<'a>> for LabelValueEncoder<'a> {
    fn from(e: protobuf::LabelValueEncoder<'a>) -> Self {
        LabelValueEncoder(LabelValueEncoderInner::Protobuf(e))
    }
}

#[derive(Debug)]
enum LabelValueEncoderInner<'a> {
    Text(text::LabelValueEncoder<'a>),
    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::LabelValueEncoder<'a>),
}

impl<'a> LabelValueEncoder<'a> {
    /// Finish encoding the label value.
    pub fn finish(self) -> Result<(), std::fmt::Error> {
        for_both!(self, LabelValueEncoderInner, e, e.finish())
    }
}

impl<'a> std::fmt::Write for LabelValueEncoder<'a> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for_both_mut!(self, LabelValueEncoderInner, e, e.write_str(s))
    }
}

impl EncodeLabelValue for &str {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.write_str(self)?;
        Ok(())
    }
}
impl EncodeLabelValue for String {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelValue::encode(&self.as_str(), encoder)
    }
}

impl<'a> EncodeLabelValue for Cow<'a, str> {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelValue::encode(&self.as_ref(), encoder)
    }
}

impl EncodeLabelValue for f64 {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.write_str(dtoa::Buffer::new().format(*self))
    }
}

impl<T> EncodeLabelValue for Option<T>
where
    T: EncodeLabelValue,
{
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
        match self {
            Some(v) => EncodeLabelValue::encode(v, encoder),
            None => EncodeLabelValue::encode(&"", encoder),
        }
    }
}

macro_rules! impl_encode_label_value_for_integer {
    ($($t:ident),*) => {$(
        impl EncodeLabelValue for $t {
            fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
                encoder.write_str(itoa::Buffer::new().format(*self))
            }
        }
    )*};
}

impl_encode_label_value_for_integer!(
    u128, i128, u64, i64, u32, i32, u16, i16, u8, i8, usize, isize
);

/// An encodable gauge value.
pub trait EncodeGaugeValue {
    /// Encode the given instance in the OpenMetrics text encoding.
    fn encode(&self, encoder: &mut GaugeValueEncoder) -> Result<(), std::fmt::Error>;
}

impl EncodeGaugeValue for i64 {
    fn encode(&self, encoder: &mut GaugeValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_i64(*self)
    }
}

impl EncodeGaugeValue for f64 {
    fn encode(&self, encoder: &mut GaugeValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_f64(*self)
    }
}

/// Encoder for a gauge value.
#[derive(Debug)]
pub struct GaugeValueEncoder<'a>(GaugeValueEncoderInner<'a>);

#[derive(Debug)]
enum GaugeValueEncoderInner<'a> {
    Text(text::GaugeValueEncoder<'a>),
    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::GaugeValueEncoder<'a>),
}

impl<'a> GaugeValueEncoder<'a> {
    fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, GaugeValueEncoderInner, e, e.encode_f64(v))
    }

    fn encode_i64(&mut self, v: i64) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, GaugeValueEncoderInner, e, e.encode_i64(v))
    }
}

impl<'a> From<text::GaugeValueEncoder<'a>> for GaugeValueEncoder<'a> {
    fn from(e: text::GaugeValueEncoder<'a>) -> Self {
        GaugeValueEncoder(GaugeValueEncoderInner::Text(e))
    }
}

#[cfg(feature = "protobuf")]
impl<'a> From<protobuf::GaugeValueEncoder<'a>> for GaugeValueEncoder<'a> {
    fn from(e: protobuf::GaugeValueEncoder<'a>) -> Self {
        GaugeValueEncoder(GaugeValueEncoderInner::Protobuf(e))
    }
}

/// An encodable counter value.
pub trait EncodeCounterValue {
    /// Encode the given instance in the OpenMetrics text encoding.
    fn encode(&self, encoder: &mut CounterValueEncoder) -> Result<(), std::fmt::Error>;
}

impl EncodeCounterValue for u64 {
    fn encode(&self, encoder: &mut CounterValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_u64(*self)
    }
}

impl EncodeCounterValue for f64 {
    fn encode(&self, encoder: &mut CounterValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_f64(*self)
    }
}

/// Encoder for a counter value.
#[derive(Debug)]
pub struct CounterValueEncoder<'a>(CounterValueEncoderInner<'a>);

#[derive(Debug)]
enum CounterValueEncoderInner<'a> {
    Text(text::CounterValueEncoder<'a>),
    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::CounterValueEncoder<'a>),
}

impl<'a> CounterValueEncoder<'a> {
    fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, CounterValueEncoderInner, e, e.encode_f64(v))
    }

    fn encode_u64(&mut self, v: u64) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, CounterValueEncoderInner, e, e.encode_u64(v))
    }
}

/// An encodable exemplar value.
pub trait EncodeExemplarValue {
    /// Encode the given instance in the OpenMetrics text encoding.
    fn encode(&self, encoder: ExemplarValueEncoder) -> Result<(), std::fmt::Error>;
}

impl EncodeExemplarValue for f64 {
    fn encode(&self, mut encoder: ExemplarValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode(*self)
    }
}

impl EncodeExemplarValue for u64 {
    fn encode(&self, mut encoder: ExemplarValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode(*self as f64)
    }
}

impl<'a> From<text::CounterValueEncoder<'a>> for CounterValueEncoder<'a> {
    fn from(e: text::CounterValueEncoder<'a>) -> Self {
        CounterValueEncoder(CounterValueEncoderInner::Text(e))
    }
}

#[cfg(feature = "protobuf")]
impl<'a> From<protobuf::CounterValueEncoder<'a>> for CounterValueEncoder<'a> {
    fn from(e: protobuf::CounterValueEncoder<'a>) -> Self {
        CounterValueEncoder(CounterValueEncoderInner::Protobuf(e))
    }
}

/// Encoder for an exemplar value.
#[derive(Debug)]
pub struct ExemplarValueEncoder<'a>(ExemplarValueEncoderInner<'a>);

#[derive(Debug)]
enum ExemplarValueEncoderInner<'a> {
    Text(text::ExemplarValueEncoder<'a>),
    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::ExemplarValueEncoder<'a>),
}

impl<'a> ExemplarValueEncoder<'a> {
    fn encode(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, ExemplarValueEncoderInner, e, e.encode(v))
    }
}

impl<'a> From<text::ExemplarValueEncoder<'a>> for ExemplarValueEncoder<'a> {
    fn from(e: text::ExemplarValueEncoder<'a>) -> Self {
        ExemplarValueEncoder(ExemplarValueEncoderInner::Text(e))
    }
}

#[cfg(feature = "protobuf")]
impl<'a> From<protobuf::ExemplarValueEncoder<'a>> for ExemplarValueEncoder<'a> {
    fn from(e: protobuf::ExemplarValueEncoder<'a>) -> Self {
        ExemplarValueEncoder(ExemplarValueEncoderInner::Protobuf(e))
    }
}
