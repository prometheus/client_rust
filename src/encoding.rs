//! Exposition format implementations.

pub use prometheus_client_derive_encode::*;

use crate::metrics::exemplar::Exemplar;
use crate::metrics::MetricType;
use crate::registry::{Prefix, Unit};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

#[cfg(feature = "protobuf")]
#[cfg_attr(docsrs, doc(cfg(feature = "protobuf")))]
pub mod protobuf;
pub mod text;

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

/// Trait implemented by each metric type, e.g.
/// [`Counter`](crate::metrics::counter::Counter), to implement its encoding in
/// the OpenMetric text format.
pub trait EncodeMetric {
    /// Encode the given instance in the OpenMetrics text encoding.
    // TODO: Lifetimes on MetricEncoder needed?
    fn encode(&self, encoder: MetricEncoder) -> Result<(), std::fmt::Error>;

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

/// Encoder for a Metric Descriptor.
#[derive(Debug)]
pub struct DescriptorEncoder<'a>(DescriptorEncoderInner<'a>);

#[derive(Debug)]
enum DescriptorEncoderInner<'a> {
    Text(text::DescriptorEncoder<'a>),

    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::DescriptorEncoder<'a>),
}

impl<'a> From<text::DescriptorEncoder<'a>> for DescriptorEncoder<'a> {
    fn from(e: text::DescriptorEncoder<'a>) -> Self {
        Self(DescriptorEncoderInner::Text(e))
    }
}

#[cfg(feature = "protobuf")]
impl<'a> From<protobuf::DescriptorEncoder<'a>> for DescriptorEncoder<'a> {
    fn from(e: protobuf::DescriptorEncoder<'a>) -> Self {
        Self(DescriptorEncoderInner::Protobuf(e))
    }
}

impl DescriptorEncoder<'_> {
    pub(crate) fn with_prefix_and_labels<'s>(
        &'s mut self,
        prefix: Option<&'s Prefix>,
        labels: &'s [(Cow<'static, str>, Cow<'static, str>)],
        // TODO: result needed?
    ) -> DescriptorEncoder<'s> {
        for_both_mut!(
            self,
            DescriptorEncoderInner,
            e,
            e.with_prefix_and_labels(prefix, labels).into()
        )
    }

    /// Encode a descriptor.
    pub fn encode_descriptor<'s>(
        &'s mut self,
        name: &'s str,
        help: &str,
        unit: Option<&'s Unit>,
        metric_type: MetricType,
    ) -> Result<MetricEncoder<'s>, std::fmt::Error> {
        for_both_mut!(
            self,
            DescriptorEncoderInner,
            e,
            Ok(e.encode_descriptor(name, help, unit, metric_type)?.into())
        )
    }
}

/// Encoder for a metric.
#[derive(Debug)]
pub struct MetricEncoder<'a>(MetricEncoderInner<'a>);

#[derive(Debug)]
enum MetricEncoderInner<'a> {
    Text(text::MetricEncoder<'a>),

    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::MetricEncoder<'a>),
}

impl<'a> From<text::MetricEncoder<'a>> for MetricEncoder<'a> {
    fn from(e: text::MetricEncoder<'a>) -> Self {
        Self(MetricEncoderInner::Text(e))
    }
}

#[cfg(feature = "protobuf")]
impl<'a> From<protobuf::MetricEncoder<'a>> for MetricEncoder<'a> {
    fn from(e: protobuf::MetricEncoder<'a>) -> Self {
        Self(MetricEncoderInner::Protobuf(e))
    }
}

impl MetricEncoder<'_> {
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
    pub fn encode_family<'s, S: EncodeLabelSet>(
        &'s mut self,
        label_set: &'s S,
    ) -> Result<MetricEncoder<'s>, std::fmt::Error> {
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

/// Encoder for a label set.
#[derive(Debug)]
pub struct LabelSetEncoder<'a>(LabelSetEncoderInner<'a>);

#[derive(Debug)]
enum LabelSetEncoderInner<'a> {
    Text(text::LabelSetEncoder<'a>),
    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::LabelSetEncoder<'a>),
}

impl<'a> From<text::LabelSetEncoder<'a>> for LabelSetEncoder<'a> {
    fn from(e: text::LabelSetEncoder<'a>) -> Self {
        Self(LabelSetEncoderInner::Text(e))
    }
}

#[cfg(feature = "protobuf")]
impl<'a> From<protobuf::LabelSetEncoder<'a>> for LabelSetEncoder<'a> {
    fn from(e: protobuf::LabelSetEncoder<'a>) -> Self {
        Self(LabelSetEncoderInner::Protobuf(e))
    }
}

impl LabelSetEncoder<'_> {
    /// Encode the given label.
    pub fn encode_label(&mut self) -> LabelEncoder {
        for_both_mut!(self, LabelSetEncoderInner, e, e.encode_label().into())
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

/// Uninhabited type to represent the lack of a label set for a metric
#[derive(Debug)]
pub enum NoLabelSet {}

impl EncodeLabelSet for NoLabelSet {
    fn encode(&self, _encoder: LabelSetEncoder) -> Result<(), std::fmt::Error> {
        Ok(())
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

impl LabelEncoder<'_> {
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

impl std::fmt::Write for LabelKeyEncoder<'_> {
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

impl EncodeLabelKey for Cow<'_, str> {
    fn encode(&self, encoder: &mut LabelKeyEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelKey::encode(&self.as_ref(), encoder)
    }
}

impl<T> EncodeLabelKey for Box<T>
where
    for<'a> &'a T: EncodeLabelKey,
{
    fn encode(&self, encoder: &mut LabelKeyEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelKey::encode(&self.as_ref(), encoder)
    }
}

impl<T> EncodeLabelKey for Arc<T>
where
    for<'a> &'a T: EncodeLabelKey,
{
    fn encode(&self, encoder: &mut LabelKeyEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelKey::encode(&self.as_ref(), encoder)
    }
}

impl<T> EncodeLabelKey for Rc<T>
where
    for<'a> &'a T: EncodeLabelKey,
{
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

#[derive(Debug)]
enum LabelValueEncoderInner<'a> {
    Text(text::LabelValueEncoder<'a>),
    #[cfg(feature = "protobuf")]
    Protobuf(protobuf::LabelValueEncoder<'a>),
}

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

impl std::fmt::Write for LabelValueEncoder<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for_both_mut!(self, LabelValueEncoderInner, e, e.write_str(s))
    }
}

impl LabelValueEncoder<'_> {
    /// Finish encoding the label value.
    pub fn finish(self) -> Result<(), std::fmt::Error> {
        for_both!(self, LabelValueEncoderInner, e, e.finish())
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

impl EncodeLabelValue for &String {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelValue::encode(&self.as_str(), encoder)
    }
}

impl EncodeLabelValue for Cow<'_, str> {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelValue::encode(&self.as_ref(), encoder)
    }
}

impl<T> EncodeLabelValue for Box<T>
where
    for<'a> &'a T: EncodeLabelValue,
{
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelValue::encode(&self.as_ref(), encoder)
    }
}

impl<T> EncodeLabelValue for Arc<T>
where
    for<'a> &'a T: EncodeLabelValue,
{
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
        EncodeLabelValue::encode(&self.as_ref(), encoder)
    }
}

impl<T> EncodeLabelValue for Rc<T>
where
    for<'a> &'a T: EncodeLabelValue,
{
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

impl EncodeLabelValue for bool {
    fn encode(&self, encoder: &mut LabelValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.write_str(if *self { "true" } else { "false" })
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

impl EncodeGaugeValue for u32 {
    fn encode(&self, encoder: &mut GaugeValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_u32(*self)
    }
}

impl EncodeGaugeValue for i64 {
    fn encode(&self, encoder: &mut GaugeValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_i64(*self)
    }
}

impl EncodeGaugeValue for u64 {
    fn encode(&self, encoder: &mut GaugeValueEncoder) -> Result<(), std::fmt::Error> {
        // Between forcing end users to do endless as i64 for things that are
        // clearly u64 and having one error case for rarely used protobuf when
        // a gauge is set to u64::MAX, the latter seems like the right choice.
        if *self == u64::MAX {
            return Err(std::fmt::Error);
        }

        encoder.encode_i64(*self as i64)
    }
}

impl EncodeGaugeValue for f64 {
    fn encode(&self, encoder: &mut GaugeValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_f64(*self)
    }
}

impl EncodeGaugeValue for i32 {
    fn encode(&self, encoder: &mut GaugeValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_i64(*self as i64)
    }
}

impl EncodeGaugeValue for f32 {
    fn encode(&self, encoder: &mut GaugeValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_f64(*self as f64)
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

impl GaugeValueEncoder<'_> {
    fn encode_u32(&mut self, v: u32) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, GaugeValueEncoderInner, e, e.encode_u32(v))
    }

    fn encode_i64(&mut self, v: i64) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, GaugeValueEncoderInner, e, e.encode_i64(v))
    }

    fn encode_f64(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, GaugeValueEncoderInner, e, e.encode_f64(v))
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

impl EncodeCounterValue for u32 {
    fn encode(&self, encoder: &mut CounterValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_u64(*self as u64)
    }
}

impl EncodeCounterValue for f32 {
    fn encode(&self, encoder: &mut CounterValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode_f64(*self as f64)
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

impl CounterValueEncoder<'_> {
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

impl EncodeExemplarValue for f32 {
    fn encode(&self, mut encoder: ExemplarValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode(*self as f64)
    }
}

impl EncodeExemplarValue for u32 {
    fn encode(&self, mut encoder: ExemplarValueEncoder) -> Result<(), std::fmt::Error> {
        encoder.encode(*self as f64)
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

impl ExemplarValueEncoder<'_> {
    fn encode(&mut self, v: f64) -> Result<(), std::fmt::Error> {
        for_both_mut!(self, ExemplarValueEncoderInner, e, e.encode(v))
    }
}

/// Enum for determining how metric and label names will
/// be validated.
#[derive(Debug, PartialEq, Default, Clone, Copy)]
pub enum ValidationScheme {
    /// Setting that requires that metric and label names
    /// conform to the original OpenMetrics character requirements.
    #[default]
    LegacyValidation,
    /// Only requires that metric and label names be valid UTF-8
    /// strings.
    UTF8Validation,
}

fn is_valid_legacy_char(c: char, i: usize) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == ':' || (c.is_ascii_digit() && i > 0)
}

fn is_valid_legacy_metric_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    for (i, c) in name.chars().enumerate() {
        if !is_valid_legacy_char(c, i) {
            return false;
        }
    }
    true
}

fn is_valid_legacy_prefix(prefix: Option<&Prefix>) -> bool {
    match prefix {
        Some(prefix) => is_valid_legacy_metric_name(prefix.as_str()),
        None => true,
    }
}

fn is_quoted_metric_name(
    name: &str,
    prefix: Option<&Prefix>,
    validation_scheme: ValidationScheme,
) -> bool {
    validation_scheme == ValidationScheme::UTF8Validation
        && (!is_valid_legacy_metric_name(name) || !is_valid_legacy_prefix(prefix))
}

fn is_valid_legacy_label_name(label_name: &str) -> bool {
    if label_name.is_empty() {
        return false;
    }
    for (i, b) in label_name.chars().enumerate() {
        if !(b.is_ascii_alphabetic() || b == '_' || (b.is_ascii_digit() && i > 0)) {
            return false;
        }
    }
    true
}

fn is_quoted_label_name(name: &str, validation_scheme: ValidationScheme) -> bool {
    validation_scheme == ValidationScheme::UTF8Validation && !is_valid_legacy_label_name(name)
}

/// Enum for determining how metric and label names will
/// be escaped.
#[derive(Debug, Default, Clone, Copy)]
pub enum EscapingScheme {
    /// Replaces all legacy-invalid characters with underscores.
    #[default]
    UnderscoreEscaping,
    /// Similar to UnderscoreEscaping, except that dots are
    /// converted to `_dot_` and pre-existing underscores are converted to `__`.
    DotsEscaping,
    /// Prepends the name with `U__` and replaces all invalid
    /// characters with the Unicode value, surrounded by underscores. Single
    /// underscores are replaced with double underscores.
    ValueEncodingEscaping,
    /// Indicates that a name will not be escaped.
    NoEscaping,
}

impl EscapingScheme {
    /// Returns a string representation of a `EscapingScheme`.
    pub fn as_str(&self) -> &str {
        match self {
            EscapingScheme::UnderscoreEscaping => "underscores",
            EscapingScheme::DotsEscaping => "dots",
            EscapingScheme::ValueEncodingEscaping => "values",
            EscapingScheme::NoEscaping => "allow-utf-8",
        }
    }
}

fn escape_name(name: &str, scheme: EscapingScheme) -> Cow<'_, str> {
    if name.is_empty() {
        return name.into();
    }
    match scheme {
        EscapingScheme::NoEscaping => name.into(),
        EscapingScheme::UnderscoreEscaping | EscapingScheme::ValueEncodingEscaping
            if is_valid_legacy_metric_name(name) =>
        {
            name.into()
        }
        EscapingScheme::UnderscoreEscaping => {
            let mut escaped = String::with_capacity(name.len());
            for (i, b) in name.chars().enumerate() {
                if is_valid_legacy_char(b, i) {
                    escaped.push(b);
                } else {
                    escaped.push('_');
                }
            }
            escaped.into()
        }
        EscapingScheme::DotsEscaping => {
            let mut escaped = String::with_capacity(name.len());
            for (i, b) in name.chars().enumerate() {
                if b == '_' {
                    escaped.push_str("__");
                } else if b == '.' {
                    escaped.push_str("_dot_");
                } else if is_valid_legacy_char(b, i) {
                    escaped.push(b);
                } else {
                    escaped.push_str("__");
                }
            }
            escaped.into()
        }
        EscapingScheme::ValueEncodingEscaping => {
            let mut escaped = String::with_capacity(name.len());
            escaped.push_str("U__");
            for (i, b) in name.chars().enumerate() {
                if b == '_' {
                    escaped.push_str("__");
                } else if is_valid_legacy_char(b, i) {
                    escaped.push(b);
                } else {
                    write!(escaped, "_{:x}_", b as i64).unwrap();
                }
            }
            escaped.into()
        }
    }
}

/// Returns the escaping scheme to use based on the given header.
pub fn negotiate_escaping_scheme(
    header: &str,
    default_escaping_scheme: EscapingScheme,
) -> EscapingScheme {
    if header.contains("underscores") {
        return EscapingScheme::UnderscoreEscaping;
    }
    if header.contains("dots") {
        return EscapingScheme::DotsEscaping;
    }
    if header.contains("values") {
        return EscapingScheme::ValueEncodingEscaping;
    }
    if header.contains("allow-utf-8") {
        return EscapingScheme::NoEscaping;
    }
    default_escaping_scheme
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_name_is_legacy_valid() {
        struct Scenario {
            input: &'static str,
            expected: bool,
        }

        let scenarios = vec![
            Scenario {
                input: "Avalid_23name",
                expected: true,
            },
            Scenario {
                input: "_Avalid_23name",
                expected: true,
            },
            Scenario {
                input: "1valid_23name",
                expected: false,
            },
            Scenario {
                input: "avalid_23name",
                expected: true,
            },
            Scenario {
                input: "Ava:lid_23name",
                expected: true,
            },
            Scenario {
                input: "a lid_23name",
                expected: false,
            },
            Scenario {
                input: ":leading_colon",
                expected: true,
            },
            Scenario {
                input: "colon:in:the:middle",
                expected: true,
            },
            Scenario {
                input: "",
                expected: false,
            },
            Scenario {
                input: "aÅz",
                expected: false,
            },
        ];

        for scenario in scenarios {
            let result = is_valid_legacy_metric_name(scenario.input);
            assert_eq!(result, scenario.expected);
        }
    }

    #[test]
    fn label_name_is_legacy_valid() {
        struct Scenario {
            input: &'static str,
            expected: bool,
        }

        let scenarios = vec![
            Scenario {
                input: "Avalid_23name",
                expected: true,
            },
            Scenario {
                input: "_Avalid_23name",
                expected: true,
            },
            Scenario {
                input: "1valid_23name",
                expected: false,
            },
            Scenario {
                input: "avalid_23name",
                expected: true,
            },
            Scenario {
                input: "Ava:lid_23name",
                expected: false,
            },
            Scenario {
                input: "a lid_23name",
                expected: false,
            },
            Scenario {
                input: ":leading_colon",
                expected: false,
            },
            Scenario {
                input: "colon:in:the:middle",
                expected: false,
            },
            Scenario {
                input: "",
                expected: false,
            },
            Scenario {
                input: "aÅz",
                expected: false,
            },
        ];

        for scenario in scenarios {
            let result = is_valid_legacy_label_name(scenario.input);
            assert_eq!(result, scenario.expected);
        }
    }

    #[test]
    fn test_escape_name() {
        struct Scenario {
            name: &'static str,
            input: &'static str,
            expected_underscores: &'static str,
            expected_dots: &'static str,
            expected_value: &'static str,
        }

        let scenarios = vec![
            Scenario {
                name: "empty string",
                input: "",
                expected_underscores: "",
                expected_dots: "",
                expected_value: "",
            },
            Scenario {
                name: "legacy valid name",
                input: "no:escaping_required",
                expected_underscores: "no:escaping_required",
                expected_dots: "no:escaping__required",
                expected_value: "no:escaping_required",
            },
            Scenario {
                name: "name with dots",
                input: "mysystem.prod.west.cpu.load",
                expected_underscores: "mysystem_prod_west_cpu_load",
                expected_dots: "mysystem_dot_prod_dot_west_dot_cpu_dot_load",
                expected_value: "U__mysystem_2e_prod_2e_west_2e_cpu_2e_load",
            },
            Scenario {
                name: "name with dots and underscore",
                input: "mysystem.prod.west.cpu.load_total",
                expected_underscores: "mysystem_prod_west_cpu_load_total",
                expected_dots: "mysystem_dot_prod_dot_west_dot_cpu_dot_load__total",
                expected_value: "U__mysystem_2e_prod_2e_west_2e_cpu_2e_load__total",
            },
            Scenario {
                name: "name with dots and colon",
                input: "http.status:sum",
                expected_underscores: "http_status:sum",
                expected_dots: "http_dot_status:sum",
                expected_value: "U__http_2e_status:sum",
            },
            Scenario {
                name: "name with spaces and emoji",
                input: "label with 😱",
                expected_underscores: "label_with__",
                expected_dots: "label__with____",
                expected_value: "U__label_20_with_20__1f631_",
            },
            Scenario {
                name: "name with unicode characters > 0x100",
                input: "花火",
                expected_underscores: "__",
                expected_dots: "____",
                expected_value: "U___82b1__706b_",
            },
            Scenario {
                name: "name with spaces and edge-case value",
                input: "label with \u{0100}",
                expected_underscores: "label_with__",
                expected_dots: "label__with____",
                expected_value: "U__label_20_with_20__100_",
            },
        ];

        for scenario in scenarios {
            let result = escape_name(scenario.input, EscapingScheme::UnderscoreEscaping);
            assert_eq!(result, scenario.expected_underscores, "{}", scenario.name);

            let result = escape_name(scenario.input, EscapingScheme::DotsEscaping);
            assert_eq!(result, scenario.expected_dots, "{}", scenario.name);

            let result = escape_name(scenario.input, EscapingScheme::ValueEncodingEscaping);
            assert_eq!(result, scenario.expected_value, "{}", scenario.name);
        }
    }
}
