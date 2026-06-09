//! Module implementing an Open Metrics histogram.
//!
//! See [`Histogram`] for details.

use crate::encoding::{
    EncodeLabelSet, EncodeMetric, MetricEncoder, NativeHistogram, NativeHistogramBuckets,
    NoLabelSet,
};
use crate::metrics::exemplar::Exemplar;

use super::{MetricType, TypedMetric};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::iter::{self, once};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Special zero-threshold sentinel that configures a zero-width zero bucket.
pub const NATIVE_HISTOGRAM_ZERO_THRESHOLD_ZERO: f64 = -1.0;

/// Default bucket factor for native histograms.
pub const DEFAULT_NATIVE_HISTOGRAM_BUCKET_FACTOR: f64 = 1.1;

/// Default zero threshold for native histograms.
pub const DEFAULT_NATIVE_HISTOGRAM_ZERO_THRESHOLD: f64 = 2.938735877055719e-39;

const SCHEMA_MIN: i8 = -4;
const SCHEMA_MAX: i8 = 8;

/// Open Metrics [`Histogram`] to measure distributions of discrete events.
///
/// ```
/// # use prometheus_client::metrics::histogram::{Histogram, exponential_buckets};
/// let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
/// histogram.observe(4.2);
/// ```
///
/// [`Histogram`] does not implement [`Default`], given that the choice of
/// bucket values depends on the situation [`Histogram`] is used in. As an
/// example, to measure HTTP request latency, the values suggested in the
/// Golang implementation might work for you:
///
/// ```
/// # use prometheus_client::metrics::histogram::Histogram;
/// // Default values from go client(https://github.com/prometheus/client_golang/blob/5d584e2717ef525673736d72cd1d12e304f243d7/prometheus/histogram.go#L68)
/// let custom_buckets = [
///    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
/// ];
/// let histogram = Histogram::new(custom_buckets);
/// histogram.observe(4.2);
/// ```
// TODO: Consider using atomics. See
// https://github.com/tikv/rust-prometheus/pull/314.
#[derive(Debug)]
pub struct Histogram {
    inner: Arc<Mutex<Inner>>,
}

impl Clone for Histogram {
    fn clone(&self) -> Self {
        Histogram {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Inner {
    // TODO: Consider allowing integer observe values.
    sum: f64,
    count: u64,
    // TODO: Consider being generic over the bucket length.
    buckets: Vec<(f64, u64)>,
    native: Option<NativeHistogramState>,
}

/// Configuration for native histogram buckets.
///
/// Native histogram exemplars are not currently emitted.
#[derive(Clone, Copy, Debug)]
pub struct NativeHistogramConfig {
    schema: i8,
    zero_threshold: f64,
    max_buckets: usize,
    min_reset_duration: Option<Duration>,
    max_zero_threshold: f64,
}

impl NativeHistogramConfig {
    /// Create a native histogram configuration with the provided bucket factor.
    ///
    /// The schema is chosen so that the bucket growth factor is the largest
    /// supported factor that is still <= `bucket_factor`. The default zero
    /// threshold is 2^-128, as recommended by the Prometheus native histogram
    /// specification.
    pub fn new(bucket_factor: f64) -> Self {
        Self::with_bucket_factor(bucket_factor)
    }

    /// Create a native histogram configuration with the provided schema.
    ///
    /// Valid standard schema values are in [-4, 8]. Prefer [`Self::new`] unless
    /// you explicitly need a schema-level configuration.
    pub fn with_schema(schema: i8) -> Self {
        assert!((SCHEMA_MIN..=SCHEMA_MAX).contains(&schema));

        Self {
            schema,
            zero_threshold: DEFAULT_NATIVE_HISTOGRAM_ZERO_THRESHOLD,
            max_buckets: 0,
            min_reset_duration: None,
            max_zero_threshold: 0.0,
        }
    }

    /// Create a native histogram configuration by choosing the schema whose
    /// growth factor is the largest factor <= `bucket_factor`.
    pub fn with_bucket_factor(bucket_factor: f64) -> Self {
        assert!(bucket_factor > 1.0);
        Self::with_schema(pick_schema(bucket_factor))
    }

    /// Set a custom zero threshold.
    ///
    /// A value of `0.0` keeps the default threshold. A negative value configures
    /// a zero-width zero bucket.
    pub fn zero_threshold(mut self, zero_threshold: f64) -> Self {
        assert!(zero_threshold.is_finite());
        self.zero_threshold = if zero_threshold > 0.0 {
            zero_threshold
        } else if zero_threshold == 0.0 {
            DEFAULT_NATIVE_HISTOGRAM_ZERO_THRESHOLD
        } else {
            NATIVE_HISTOGRAM_ZERO_THRESHOLD_ZERO
        };
        self
    }

    /// Set a best-effort maximum number of sparse buckets across both sides.
    ///
    /// `0` means unbounded. If the lowest supported resolution still exceeds
    /// the limit, observations remain in sparse buckets until a later zero
    /// bucket widening can merge them.
    pub fn max_buckets(mut self, max_buckets: usize) -> Self {
        self.max_buckets = max_buckets;
        self
    }

    /// Reset the histogram instead of reducing resolution if the bucket limit
    /// is exceeded after at least this duration has elapsed since creation or
    /// the last reset. A zero duration disables reset-based limiting.
    pub fn min_reset_duration(mut self, min_reset_duration: Duration) -> Self {
        self.min_reset_duration =
            (min_reset_duration != Duration::ZERO).then_some(min_reset_duration);
        self
    }

    /// Set the maximum zero threshold allowed while enforcing `max_buckets`.
    ///
    /// The default is `0.0`, which disables zero-bucket widening unless the
    /// current threshold is already below a configured positive maximum.
    pub fn max_zero_threshold(mut self, max_zero_threshold: f64) -> Self {
        assert!(max_zero_threshold.is_finite());
        assert!(max_zero_threshold >= 0.0);
        self.max_zero_threshold = max_zero_threshold;
        self
    }
}

impl Default for NativeHistogramConfig {
    fn default() -> Self {
        Self::new(DEFAULT_NATIVE_HISTOGRAM_BUCKET_FACTOR)
    }
}

#[derive(Debug)]
struct NativeHistogramState {
    initial_zero_threshold: f64,
    initial_schema: i8,
    zero_threshold: f64,
    zero_count: u64,
    schema: i8,
    max_buckets: usize,
    min_reset_duration: Option<Duration>,
    max_zero_threshold: f64,
    created: SystemTime,
    scheduled_reset: Option<SystemTime>,
    positive: NativeBuckets,
    negative: NativeBuckets,
}

type NativeBuckets = Vec<(i32, u64)>;

#[derive(Debug)]
struct NativeHistogramSnapshot {
    schema: i32,
    zero_threshold: f64,
    zero_count: u64,
    negative: NativeBucketEncoding,
    positive: NativeBucketEncoding,
    created: SystemTime,
}

#[derive(Debug)]
struct NativeBucketEncoding {
    spans: Vec<(i32, u32)>,
    deltas: Vec<i64>,
}

#[derive(Debug)]
struct HistogramSnapshot {
    sum: f64,
    count: u64,
    buckets: Vec<(f64, u64)>,
    native: Option<NativeHistogramSnapshot>,
}

impl NativeHistogramState {
    fn new(config: NativeHistogramConfig) -> Self {
        let created = SystemTime::now();
        Self {
            initial_zero_threshold: config.zero_threshold,
            initial_schema: config.schema,
            zero_threshold: config.zero_threshold,
            zero_count: 0,
            schema: config.schema,
            max_buckets: config.max_buckets,
            min_reset_duration: config.min_reset_duration,
            max_zero_threshold: config.max_zero_threshold,
            created,
            scheduled_reset: None,
            positive: Vec::new(),
            negative: Vec::new(),
        }
    }

    fn observe(&mut self, v: f64) -> bool {
        if in_zero_bucket(self.zero_threshold, v) {
            self.zero_count += 1;
            return enforce_bucket_limit(self);
        }

        let index = bucket_index(self.schema, v.abs(), v.is_infinite());
        if v.is_sign_negative() {
            increment_bucket(&mut self.negative, index);
        } else {
            increment_bucket(&mut self.positive, index);
        }

        enforce_bucket_limit(self)
    }

    fn reset(&mut self, created: SystemTime) {
        self.zero_threshold = self.initial_zero_threshold;
        self.zero_count = 0;
        self.schema = self.initial_schema;
        self.created = created;
        self.scheduled_reset = None;
        self.positive.clear();
        self.negative.clear();
    }

    fn reset_is_due(&self, now: SystemTime) -> bool {
        self.scheduled_reset
            .and_then(|scheduled_reset| now.duration_since(scheduled_reset).ok())
            .is_some()
    }

    fn schedule_reset_after_degradation(&mut self) {
        if self.scheduled_reset.is_none() {
            self.scheduled_reset = self
                .min_reset_duration
                .and_then(|duration| self.created.checked_add(duration));
        }
    }

    fn snapshot(&self) -> Result<NativeHistogramSnapshot, std::fmt::Error> {
        let mut negative = encode_spans_and_deltas(&self.negative)?;
        let mut positive = encode_spans_and_deltas(&self.positive)?;

        let exported_zero_threshold = if self.zero_threshold < 0.0 {
            0.0
        } else {
            self.zero_threshold
        };

        // Distinguish a native histogram from a classic histogram in protobuf
        // when it has no sparse bucket data and zero threshold is zero.
        if exported_zero_threshold == 0.0
            && self.zero_count == 0
            && positive.spans.is_empty()
            && negative.spans.is_empty()
        {
            positive.spans.push((0, 0));
        }

        if self.negative.is_empty() {
            negative.spans.clear();
        }

        Ok(NativeHistogramSnapshot {
            schema: i32::from(self.schema),
            zero_threshold: exported_zero_threshold,
            zero_count: self.zero_count,
            negative,
            positive,
            created: self.created,
        })
    }
}

impl Histogram {
    /// Create a new [`Histogram`].
    ///
    /// ```rust
    /// # use prometheus_client::metrics::histogram::Histogram;
    /// let histogram = Histogram::new([10.0, 100.0, 1_000.0]);
    /// ```
    pub fn new(buckets: impl IntoIterator<Item = f64>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                sum: Default::default(),
                count: Default::default(),
                buckets: buckets
                    .into_iter()
                    .chain(once(f64::MAX))
                    .map(|upper_bound| (upper_bound, 0))
                    .collect(),
                native: None,
            })),
        }
    }

    /// Create a new native [`Histogram`] without classic buckets.
    ///
    /// Native-only histograms can only be encoded by the Prometheus protobuf
    /// encoder. Text and OpenMetrics protobuf encoders do not have native
    /// histogram fields and reject native-only histograms.
    pub fn new_native(native: NativeHistogramConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                sum: Default::default(),
                count: Default::default(),
                buckets: Vec::new(),
                native: Some(NativeHistogramState::new(native)),
            })),
        }
    }

    /// Create a new [`Histogram`] with both classic and native buckets.
    ///
    /// Prometheus protobuf encodes both representations in one histogram
    /// message. Encoders without native histogram support encode the classic
    /// buckets.
    pub fn new_classic_and_native(
        buckets: impl IntoIterator<Item = f64>,
        native: NativeHistogramConfig,
    ) -> Self {
        let histogram = Self::new(buckets);
        histogram.inner.lock().native = Some(NativeHistogramState::new(native));
        histogram
    }

    /// Observe the given value.
    pub fn observe(&self, v: f64) {
        self.observe_and_bucket(v);
    }

    /// Returns the current sum of all observations.
    #[cfg(any(test, feature = "test-util"))]
    pub fn sum(&self) -> f64 {
        self.inner.lock().sum
    }

    /// Returns the current number of observations.
    #[cfg(any(test, feature = "test-util"))]
    pub fn count(&self) -> u64 {
        self.inner.lock().count
    }

    /// Observes the given value, returning the index of the first bucket the
    /// value is added to.
    ///
    /// Needed in
    /// [`HistogramWithExemplars`](crate::metrics::exemplar::HistogramWithExemplars).
    pub(crate) fn observe_and_bucket(&self, v: f64) -> Option<usize> {
        let mut inner = self.inner.lock();
        reset_if_scheduled(&mut inner);
        let mut bucket = observe_classic(&mut inner, v);

        let reset = if let Some(native) = &mut inner.native {
            !v.is_nan() && native.observe(v)
        } else {
            false
        };

        if reset {
            let created = SystemTime::now();
            reset_observations(&mut inner, created);
            bucket = observe_classic(&mut inner, v);
            if let Some(native) = &mut inner.native {
                if !v.is_nan() {
                    native.observe(v);
                }
            }
        }

        bucket
    }

    fn snapshot(&self) -> Result<HistogramSnapshot, std::fmt::Error> {
        let mut inner = self.inner.lock();
        reset_if_scheduled(&mut inner);
        let native = inner
            .native
            .as_ref()
            .map(NativeHistogramState::snapshot)
            .transpose()?;

        Ok(HistogramSnapshot {
            sum: inner.sum,
            count: inner.count,
            buckets: inner.buckets.clone(),
            native,
        })
    }

    pub(crate) fn encode_with_exemplars<S: EncodeLabelSet>(
        &self,
        encoder: &mut MetricEncoder,
        exemplars: Option<&HashMap<usize, Exemplar<S, f64>>>,
    ) -> Result<(), std::fmt::Error> {
        let snapshot = self.snapshot()?;
        match snapshot.native {
            Some(native) => encoder.encode_histogram_with_native(
                snapshot.sum,
                snapshot.count,
                &snapshot.buckets,
                exemplars,
                NativeHistogram {
                    schema: native.schema,
                    zero_threshold: native.zero_threshold,
                    zero_count: native.zero_count,
                    negative: NativeHistogramBuckets {
                        spans: &native.negative.spans,
                        deltas: &native.negative.deltas,
                    },
                    positive: NativeHistogramBuckets {
                        spans: &native.positive.spans,
                        deltas: &native.positive.deltas,
                    },
                    created: Some(native.created),
                },
            ),
            None => {
                encoder.encode_histogram(snapshot.sum, snapshot.count, &snapshot.buckets, exemplars)
            }
        }
    }
}

impl TypedMetric for Histogram {
    const TYPE: MetricType = MetricType::Histogram;
}

/// Exponential bucket distribution.
pub fn exponential_buckets(start: f64, factor: f64, length: u16) -> impl Iterator<Item = f64> {
    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| start * factor.powf(i as f64))
        .take(length.into())
}

/// Exponential bucket distribution within a range
///
/// Creates `length` buckets, where the lowest bucket is `min` and the highest bucket is `max`.
///
/// If `length` is less than 1, or `min` is less than or equal to 0, an empty iterator is returned.
pub fn exponential_buckets_range(min: f64, max: f64, length: u16) -> impl Iterator<Item = f64> {
    let mut len_observed = length;
    let mut min_bucket = min;
    // length needs a positive length and min needs to be greater than 0
    // set len_observed to 0 and min_bucket to 1.0
    // this will return an empty iterator in the result
    if length < 1 || min <= 0.0 {
        len_observed = 0;
        min_bucket = 1.0;
    }
    // We know max/min and highest bucket. Solve for growth_factor.
    let growth_factor = (max / min_bucket).powf(1.0 / (len_observed as f64 - 1.0));

    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| min_bucket * growth_factor.powf(i as f64))
        .take(len_observed.into())
}

/// Linear bucket distribution.
pub fn linear_buckets(start: f64, width: f64, length: u16) -> impl Iterator<Item = f64> {
    iter::repeat(())
        .enumerate()
        .map(move |(i, _)| start + (width * (i as f64)))
        .take(length.into())
}

impl EncodeMetric for Histogram {
    fn encode(&self, mut encoder: MetricEncoder) -> Result<(), std::fmt::Error> {
        self.encode_with_exemplars::<NoLabelSet>(&mut encoder, None)
    }

    fn metric_type(&self) -> MetricType {
        Self::TYPE
    }
}

fn observe_classic(inner: &mut Inner, v: f64) -> Option<usize> {
    inner.sum += v;
    inner.count += 1;

    if inner.buckets.is_empty() {
        return None;
    }

    let first_bucket = if v.is_nan() {
        inner.buckets.iter_mut().enumerate().next_back()
    } else {
        inner
            .buckets
            .iter_mut()
            .enumerate()
            .find(|(_i, (upper_bound, _value))| upper_bound >= &v)
    };

    match first_bucket {
        Some((i, (_upper_bound, value))) => {
            *value += 1;
            Some(i)
        }
        None => None,
    }
}

fn reset_observations(inner: &mut Inner, created: SystemTime) {
    inner.sum = 0.0;
    inner.count = 0;
    for (_, count) in &mut inner.buckets {
        *count = 0;
    }
    if let Some(native) = &mut inner.native {
        native.reset(created);
    }
}

fn reset_if_scheduled(inner: &mut Inner) {
    let Some(native) = inner.native.as_ref() else {
        return;
    };
    if native.scheduled_reset.is_none() {
        return;
    }

    let now = SystemTime::now();
    if native.reset_is_due(now) {
        reset_observations(inner, now);
    }
}

fn in_zero_bucket(zero_threshold: f64, v: f64) -> bool {
    if zero_threshold < 0.0 {
        v == 0.0
    } else {
        v.abs() <= zero_threshold
    }
}

fn increment_bucket(buckets: &mut NativeBuckets, index: i32) {
    match buckets.binary_search_by_key(&index, |(bucket_index, _)| *bucket_index) {
        Ok(position) => buckets[position].1 += 1,
        Err(position) => buckets.insert(position, (index, 1)),
    }
}

fn pick_schema(bucket_factor: f64) -> i8 {
    let floor = bucket_factor.log2().log2().floor();
    if floor <= -f64::from(SCHEMA_MAX) {
        return SCHEMA_MAX;
    }

    if floor >= -f64::from(SCHEMA_MIN) {
        return SCHEMA_MIN;
    }

    (SCHEMA_MIN..=SCHEMA_MAX)
        .find(|schema| f64::from(*schema) == -floor)
        .expect("schema is in range")
}

// copied from https://github.com/prometheus/client_golang/blob/f23aad527b9740eda20fe5db147e6cd621c2c1bc/prometheus/histogram.go#L46
const NATIVE_HISTOGRAM_BOUNDS: [&[f64]; 9] = [
    &[0.5],
    &[0.5, 0.7071067811865475],
    &[
        0.5,
        0.5946035575013605,
        0.7071067811865475,
        0.8408964152537144,
    ],
    &[
        0.5,
        0.5452538663326288,
        0.5946035575013605,
        0.6484197773255048,
        0.7071067811865475,
        0.7711054127039704,
        0.8408964152537144,
        0.9170040432046711,
    ],
    &[
        0.5,
        0.5221368912137069,
        0.5452538663326288,
        0.5693943173783458,
        0.5946035575013605,
        0.620928906036742,
        0.6484197773255048,
        0.6771277734684463,
        0.7071067811865475,
        0.7384130729697496,
        0.7711054127039704,
        0.805245165974627,
        0.8408964152537144,
        0.8781260801866495,
        0.9170040432046711,
        0.9576032806985735,
    ],
    &[
        0.5,
        0.5109485743270583,
        0.5221368912137069,
        0.5335702003384117,
        0.5452538663326288,
        0.5571933712979462,
        0.5693943173783458,
        0.5818624293887887,
        0.5946035575013605,
        0.6076236799902344,
        0.620928906036742,
        0.6345254785958666,
        0.6484197773255048,
        0.6626183215798706,
        0.6771277734684463,
        0.6919549409819159,
        0.7071067811865475,
        0.7225904034885232,
        0.7384130729697496,
        0.7545822137967112,
        0.7711054127039704,
        0.7879904225539431,
        0.805245165974627,
        0.8228777390769823,
        0.8408964152537144,
        0.8593096490612387,
        0.8781260801866495,
        0.8973545375015533,
        0.9170040432046711,
        0.9370838170551498,
        0.9576032806985735,
        0.9785720620876999,
    ],
    &[
        0.5,
        0.5054446430258502,
        0.5109485743270583,
        0.5165124395106142,
        0.5221368912137069,
        0.5278225891802786,
        0.5335702003384117,
        0.5393803988785598,
        0.5452538663326288,
        0.5511912916539204,
        0.5571933712979462,
        0.5632608093041209,
        0.5693943173783458,
        0.5755946149764913,
        0.5818624293887887,
        0.5881984958251406,
        0.5946035575013605,
        0.6010783657263515,
        0.6076236799902344,
        0.6142402680534349,
        0.620928906036742,
        0.6276903785123455,
        0.6345254785958666,
        0.6414350080393891,
        0.6484197773255048,
        0.6554806057623822,
        0.6626183215798706,
        0.6698337620266515,
        0.6771277734684463,
        0.6845012114872953,
        0.6919549409819159,
        0.6994898362691555,
        0.7071067811865475,
        0.7148066691959849,
        0.7225904034885232,
        0.7304588970903234,
        0.7384130729697496,
        0.7464538641456323,
        0.7545822137967112,
        0.762799075372269,
        0.7711054127039704,
        0.7795022001189185,
        0.7879904225539431,
        0.7965710756711334,
        0.805245165974627,
        0.8140137109286738,
        0.8228777390769823,
        0.8318382901633681,
        0.8408964152537144,
        0.8500531768592616,
        0.8593096490612387,
        0.8686669176368529,
        0.8781260801866495,
        0.8876882462632604,
        0.8973545375015533,
        0.9071260877501991,
        0.9170040432046711,
        0.9269895625416926,
        0.9370838170551498,
        0.9472879907934827,
        0.9576032806985735,
        0.9680308967461471,
        0.9785720620876999,
        0.9892280131939752,
    ],
    &[
        0.5,
        0.5027149505564014,
        0.5054446430258502,
        0.5081891574554764,
        0.5109485743270583,
        0.5137229745593818,
        0.5165124395106142,
        0.5193170509806894,
        0.5221368912137069,
        0.5249720429003435,
        0.5278225891802786,
        0.5306886136446309,
        0.5335702003384117,
        0.5364674337629877,
        0.5393803988785598,
        0.5423091811066545,
        0.5452538663326288,
        0.5482145409081883,
        0.5511912916539204,
        0.5541842058618393,
        0.5571933712979462,
        0.5602188762048033,
        0.5632608093041209,
        0.5663192597993595,
        0.5693943173783458,
        0.572486072215902,
        0.5755946149764913,
        0.5787200368168754,
        0.5818624293887887,
        0.585021884841625,
        0.5881984958251406,
        0.5913923554921704,
        0.5946035575013605,
        0.5978321960199137,
        0.6010783657263515,
        0.6043421618132907,
        0.6076236799902344,
        0.6109230164863786,
        0.6142402680534349,
        0.6175755319684665,
        0.620928906036742,
        0.6243004885946023,
        0.6276903785123455,
        0.6310986751971253,
        0.6345254785958666,
        0.637970889198196,
        0.6414350080393891,
        0.6449179367033329,
        0.6484197773255048,
        0.6519406325959679,
        0.6554806057623822,
        0.659039800633032,
        0.6626183215798706,
        0.6662162735415805,
        0.6698337620266515,
        0.6734708931164728,
        0.6771277734684463,
        0.6808045103191123,
        0.6845012114872953,
        0.688217985377265,
        0.6919549409819159,
        0.6957121878859629,
        0.6994898362691555,
        0.7032879969095076,
        0.7071067811865475,
        0.7109463010845827,
        0.7148066691959849,
        0.718687998724491,
        0.7225904034885232,
        0.7265139979245261,
        0.7304588970903234,
        0.7344252166684908,
        0.7384130729697496,
        0.7424225829363761,
        0.7464538641456323,
        0.7505070348132126,
        0.7545822137967112,
        0.7586795205991071,
        0.762799075372269,
        0.7669409989204777,
        0.7711054127039704,
        0.7752924388424999,
        0.7795022001189185,
        0.7837348199827764,
        0.7879904225539431,
        0.7922691326262467,
        0.7965710756711334,
        0.8008963778413465,
        0.805245165974627,
        0.8096175675974316,
        0.8140137109286738,
        0.8184337248834821,
        0.8228777390769823,
        0.8273458838280969,
        0.8318382901633681,
        0.8363550898207981,
        0.8408964152537144,
        0.8454623996346523,
        0.8500531768592616,
        0.8546688815502312,
        0.8593096490612387,
        0.8639756154809185,
        0.8686669176368529,
        0.8733836930995842,
        0.8781260801866495,
        0.8828942179666361,
        0.8876882462632604,
        0.8925083056594671,
        0.8973545375015533,
        0.9022270839033115,
        0.9071260877501991,
        0.9120516927035263,
        0.9170040432046711,
        0.9219832844793128,
        0.9269895625416926,
        0.9320230241988943,
        0.9370838170551498,
        0.9421720895161669,
        0.9472879907934827,
        0.9524316709088368,
        0.9576032806985735,
        0.9628029718180622,
        0.9680308967461471,
        0.9732872087896164,
        0.9785720620876999,
        0.9838856116165875,
        0.9892280131939752,
        0.9945994234836328,
    ],
    &[
        0.5,
        0.5013556375251013,
        0.5027149505564014,
        0.5040779490592088,
        0.5054446430258502,
        0.5068150424757447,
        0.5081891574554764,
        0.509566998038869,
        0.5109485743270583,
        0.5123338964485679,
        0.5137229745593818,
        0.5151158188430205,
        0.5165124395106142,
        0.5179128468009786,
        0.5193170509806894,
        0.520725062344158,
        0.5221368912137069,
        0.5235525479396449,
        0.5249720429003435,
        0.526395386502313,
        0.5278225891802786,
        0.5292536613972564,
        0.5306886136446309,
        0.5321274564422321,
        0.5335702003384117,
        0.5350168559101208,
        0.5364674337629877,
        0.5379219445313954,
        0.5393803988785598,
        0.5408428074966075,
        0.5423091811066545,
        0.5437795304588847,
        0.5452538663326288,
        0.5467321995364429,
        0.5482145409081883,
        0.549700901315111,
        0.5511912916539204,
        0.5526857228508706,
        0.5541842058618393,
        0.5556867516724088,
        0.5571933712979462,
        0.5587040757836845,
        0.5602188762048033,
        0.5617377836665098,
        0.5632608093041209,
        0.564787964283144,
        0.5663192597993595,
        0.5678547070789026,
        0.5693943173783458,
        0.5709381019847808,
        0.572486072215902,
        0.5740382394200894,
        0.5755946149764913,
        0.5771552102951081,
        0.5787200368168754,
        0.5802891060137493,
        0.5818624293887887,
        0.5834400184762408,
        0.585021884841625,
        0.5866080400818185,
        0.5881984958251406,
        0.5897932637314379,
        0.5913923554921704,
        0.5929957828304968,
        0.5946035575013605,
        0.5962156912915756,
        0.5978321960199137,
        0.5994530835371903,
        0.6010783657263515,
        0.6027080545025619,
        0.6043421618132907,
        0.6059806996384005,
        0.6076236799902344,
        0.6092711149137041,
        0.6109230164863786,
        0.6125793968185725,
        0.6142402680534349,
        0.6159056423670379,
        0.6175755319684665,
        0.6192499490999082,
        0.620928906036742,
        0.622612415087629,
        0.6243004885946023,
        0.6259931389331581,
        0.6276903785123455,
        0.6293922197748583,
        0.6310986751971253,
        0.6328097572894031,
        0.6345254785958666,
        0.6362458516947014,
        0.637970889198196,
        0.6397006037528346,
        0.6414350080393891,
        0.6431741147730128,
        0.6449179367033329,
        0.6466664866145447,
        0.6484197773255048,
        0.6501778216898253,
        0.6519406325959679,
        0.6537082229673385,
        0.6554806057623822,
        0.6572577939746774,
        0.659039800633032,
        0.6608266388015788,
        0.6626183215798706,
        0.6644148621029772,
        0.6662162735415805,
        0.6680225691020727,
        0.6698337620266515,
        0.6716498655934177,
        0.6734708931164728,
        0.6752968579460171,
        0.6771277734684463,
        0.6789636531064505,
        0.6808045103191123,
        0.6826503586020058,
        0.6845012114872953,
        0.6863570825438342,
        0.688217985377265,
        0.690083933630119,
        0.6919549409819159,
        0.6938310211492645,
        0.6957121878859629,
        0.6975984549830999,
        0.6994898362691555,
        0.7013863456101023,
        0.7032879969095076,
        0.7051948041086352,
        0.7071067811865475,
        0.7090239421602076,
        0.7109463010845827,
        0.7128738720527471,
        0.7148066691959849,
        0.7167447066838943,
        0.718687998724491,
        0.7206365595643126,
        0.7225904034885232,
        0.7245495448210174,
        0.7265139979245261,
        0.7284837772007218,
        0.7304588970903234,
        0.7324393720732029,
        0.7344252166684908,
        0.7364164454346837,
        0.7384130729697496,
        0.7404151139112358,
        0.7424225829363761,
        0.7444354947621984,
        0.7464538641456323,
        0.7484777058836176,
        0.7505070348132126,
        0.7525418658117031,
        0.7545822137967112,
        0.7566280937263048,
        0.7586795205991071,
        0.7607365094544071,
        0.762799075372269,
        0.7648672334736434,
        0.7669409989204777,
        0.7690203869158282,
        0.7711054127039704,
        0.7731960915705107,
        0.7752924388424999,
        0.7773944698885442,
        0.7795022001189185,
        0.7816156449856788,
        0.7837348199827764,
        0.7858597406461707,
        0.7879904225539431,
        0.7901268813264122,
        0.7922691326262467,
        0.7944171921585818,
        0.7965710756711334,
        0.7987307989543135,
        0.8008963778413465,
        0.8030678282083853,
        0.805245165974627,
        0.8074284071024302,
        0.8096175675974316,
        0.8118126635086642,
        0.8140137109286738,
        0.8162207259936375,
        0.8184337248834821,
        0.820652723822003,
        0.8228777390769823,
        0.8251087869603088,
        0.8273458838280969,
        0.8295890460808079,
        0.8318382901633681,
        0.8340936325652911,
        0.8363550898207981,
        0.8386226785089391,
        0.8408964152537144,
        0.8431763167241966,
        0.8454623996346523,
        0.8477546807446661,
        0.8500531768592616,
        0.8523579048290255,
        0.8546688815502312,
        0.8569861239649629,
        0.8593096490612387,
        0.8616394738731368,
        0.8639756154809185,
        0.8663180910111553,
        0.8686669176368529,
        0.871022112577578,
        0.8733836930995842,
        0.8757516765159389,
        0.8781260801866495,
        0.8805069215187917,
        0.8828942179666361,
        0.8852879870317771,
        0.8876882462632604,
        0.890095013257712,
        0.8925083056594671,
        0.8949281411607002,
        0.8973545375015533,
        0.8997875124702672,
        0.9022270839033115,
        0.9046732696855155,
        0.9071260877501991,
        0.909585556079304,
        0.9120516927035263,
        0.9145245157024483,
        0.9170040432046711,
        0.9194902933879467,
        0.9219832844793128,
        0.9244830347552253,
        0.9269895625416926,
        0.92950288621441,
        0.9320230241988943,
        0.9345499949706191,
        0.9370838170551498,
        0.93962450902828,
        0.9421720895161669,
        0.9447265771954693,
        0.9472879907934827,
        0.9498563490882775,
        0.9524316709088368,
        0.9550139751351947,
        0.9576032806985735,
        0.9601996065815236,
        0.9628029718180622,
        0.9654133954938133,
        0.9680308967461471,
        0.9706554947643201,
        0.9732872087896164,
        0.9759260581154889,
        0.9785720620876999,
        0.9812252401044634,
        0.9838856116165875,
        0.9865531961276168,
        0.9892280131939752,
        0.9919100824251095,
        0.9945994234836328,
        0.9972960560854698,
    ],
];

fn bucket_index(schema: i8, v: f64, is_infinite: bool) -> i32 {
    let v = if is_infinite { f64::MAX } else { v };
    let (frac, exp) = frexp(v);
    let mut index = if schema > 0 {
        let bounds = NATIVE_HISTOGRAM_BOUNDS
            [usize::try_from(schema).expect("positive schema can be indexed")];
        let bucket =
            i32::try_from(bounds.partition_point(|bound| *bound < frac)).expect("bounds fit i32");
        bucket + (exp - 1) * i32::try_from(bounds.len()).expect("bounds length fits i32")
    } else {
        let mut key = exp;
        if frac == 0.5 {
            key -= 1;
        }
        let shift = u32::try_from(-i32::from(schema)).expect("non-positive schema shift");
        let offset = (1_i32 << shift) - 1;
        (key + offset) >> shift
    };

    if is_infinite {
        index = index.saturating_add(1);
    }
    index
}

fn frexp(v: f64) -> (f64, i32) {
    debug_assert!(v >= 0.0);
    debug_assert!(!v.is_nan());

    if v == 0.0 {
        return (0.0, 0);
    }

    let bits = v.to_bits();
    let exponent = i32::try_from((bits >> 52) & 0x7ff).expect("f64 exponent fits i32");
    let mantissa = bits & ((1_u64 << 52) - 1);

    if exponent == 0 {
        let p = 63 - i32::try_from(mantissa.leading_zeros()).expect("leading zeros fit i32");
        let frac = mantissa as f64 / 2f64.powi(p + 1);
        (frac, p - 1073)
    } else {
        let frac = ((1_u64 << 52) | mantissa) as f64 / 2f64.powi(53);
        (frac, exponent - 1022)
    }
}

fn enforce_bucket_limit(inner: &mut NativeHistogramState) -> bool {
    if inner.max_buckets == 0 {
        return false;
    }

    if inner.positive.len() + inner.negative.len() <= inner.max_buckets {
        return false;
    }

    if inner.scheduled_reset.is_none()
        && inner
            .min_reset_duration
            .and_then(|min_reset_duration| {
                inner
                    .created
                    .elapsed()
                    .ok()
                    .map(|e| e >= min_reset_duration)
            })
            .unwrap_or(false)
    {
        return true;
    }

    if widen_zero_bucket(inner) {
        inner.schedule_reset_after_degradation();
        return false;
    }

    if inner.schema > SCHEMA_MIN {
        inner.schema -= 1;
        inner.positive = downsample_buckets(&inner.positive);
        inner.negative = downsample_buckets(&inner.negative);
        inner.schedule_reset_after_degradation();
    }

    false
}

fn widen_zero_bucket(inner: &mut NativeHistogramState) -> bool {
    let smallest_key = match (
        inner.positive.first().map(|(index, _)| *index),
        inner.negative.first().map(|(index, _)| *index),
    ) {
        (Some(positive), Some(negative)) => positive.min(negative),
        (Some(positive), None) => positive,
        (None, Some(negative)) => negative,
        (None, None) => return false,
    };

    let new_threshold = positive_upper_bound(inner.schema, smallest_key);
    let current_threshold = if inner.zero_threshold < 0.0 {
        0.0
    } else {
        inner.zero_threshold
    };
    if new_threshold <= current_threshold || new_threshold > inner.max_zero_threshold {
        return false;
    }

    let moved_positive = move_to_zero_bucket(inner.schema, new_threshold, &mut inner.positive);
    let moved_negative = move_to_zero_bucket(inner.schema, new_threshold, &mut inner.negative);

    let moved = moved_positive + moved_negative;
    if moved == 0 {
        return false;
    }

    inner.zero_count += moved;
    inner.zero_threshold = new_threshold;
    true
}

fn move_to_zero_bucket(schema: i8, threshold: f64, buckets: &mut NativeBuckets) -> u64 {
    if buckets.is_empty() {
        return 0;
    }

    let mut split = 0;
    for (index, _count) in buckets.iter() {
        let upper = positive_upper_bound(schema, *index);
        if upper <= threshold {
            split += 1;
        } else {
            break;
        }
    }

    let moved = buckets[..split].iter().map(|(_, count)| *count).sum();
    buckets.drain(..split);

    moved
}

fn positive_upper_bound(schema: i8, index: i32) -> f64 {
    if schema < 0 {
        let exp = index << u32::try_from(-i32::from(schema)).expect("negative schema shift");
        if exp == 1024 {
            return f64::MAX;
        }
        return 2f64.powi(exp);
    }

    let schema = u32::try_from(schema).expect("non-negative schema shift");
    let bounds = NATIVE_HISTOGRAM_BOUNDS[usize::try_from(schema).expect("schema can be indexed")];
    let frac = bounds[usize::try_from(index & ((1 << schema) - 1)).expect("index can be indexed")];
    let exp = (index >> schema) + 1;
    if frac == 0.5 && exp == 1025 {
        return f64::MAX;
    }
    frac * 2f64.powi(exp)
}

fn downsample_buckets(buckets: &NativeBuckets) -> NativeBuckets {
    let mut downsampled: NativeBuckets = Vec::with_capacity(buckets.len());
    for (index, count) in buckets {
        let mut key = *index;
        if key > 0 {
            key += 1;
        }
        key /= 2;
        if let Some((last_key, last_count)) = downsampled.last_mut() {
            if *last_key == key {
                *last_count += *count;
                continue;
            }
        }
        downsampled.push((key, *count));
    }
    downsampled
}

fn encode_spans_and_deltas(
    buckets: &NativeBuckets,
) -> Result<NativeBucketEncoding, std::fmt::Error> {
    let mut deltas = Vec::with_capacity(buckets.len());
    let mut previous_count = 0i64;
    let mut next_index = 0i32;
    let mut spans: Vec<(i32, u32)> = Vec::new();

    let mut append_delta = |count: i64, spans: &mut Vec<(i32, u32)>| {
        if let Some((_, len)) = spans.last_mut() {
            *len += 1;
        }
        deltas.push(count - previous_count);
        previous_count = count;
    };

    for (n, &(index, count)) in buckets.iter().enumerate() {
        let count = i64::try_from(count).map_err(|_| std::fmt::Error)?;
        let index_delta = index - next_index;

        if n == 0 || index_delta > 2 {
            spans.push((index_delta, 0));
        } else {
            for _ in 0..index_delta {
                append_delta(0, &mut spans);
            }
        }

        append_delta(count, &mut spans);
        next_index = index + 1;
    }

    Ok(NativeBucketEncoding { spans, deltas })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram() {
        let histogram = Histogram::new(exponential_buckets(1.0, 2.0, 10));
        histogram.observe(1.0);
    }

    #[test]
    fn exponential() {
        assert_eq!(
            vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0],
            exponential_buckets(1.0, 2.0, 10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn linear() {
        assert_eq!(
            vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
            linear_buckets(0.0, 1.0, 10).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exponential_range() {
        assert_eq!(
            vec![1.0, 2.0, 4.0, 8.0, 16.0, 32.0],
            exponential_buckets_range(1.0, 32.0, 6).collect::<Vec<_>>()
        );
    }

    #[test]
    fn exponential_range_incorrect() {
        let res = exponential_buckets_range(1.0, 32.0, 0).collect::<Vec<_>>();
        assert!(res.is_empty());

        let res = exponential_buckets_range(0.0, 32.0, 6).collect::<Vec<_>>();
        assert!(res.is_empty());
    }

    /// Checks that [`Histogram::count()`] works properly.
    #[test]
    fn count() {
        let histogram = Histogram::new([1.0_f64, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(
            histogram.count(),
            0,
            "histogram has zero observations when instantiated"
        );

        histogram.observe(1.0);
        assert_eq!(histogram.count(), 1, "histogram has one observation");

        histogram.observe(2.5);
        assert_eq!(histogram.count(), 2, "histogram has two observations");

        histogram.observe(6.0);
        assert_eq!(histogram.count(), 3, "histogram has three observations");
    }

    /// Checks that [`Histogram::sum()`] works properly.
    #[test]
    fn sum() {
        const BUCKETS: [f64; 3] = [10.0, 100.0, 1000.0];
        let histogram = Histogram::new(BUCKETS);
        assert_eq!(
            histogram.sum(),
            0.0,
            "histogram sum is zero when instantiated"
        );

        histogram.observe(3.0); // 3 + 4 + 15 + 101 = 123
        histogram.observe(4.0);
        histogram.observe(15.0);
        histogram.observe(101.0);
        assert_eq!(
            histogram.sum(),
            123.0,
            "histogram sum records accurate sum of observations"
        );

        histogram.observe(1111.0);
        assert_eq!(
            histogram.sum(),
            1234.0,
            "histogram sum records accurate sum of observations"
        );
    }

    #[test]
    fn native_histogram_stores_sparse_buckets() {
        let h = Histogram::new_native(NativeHistogramConfig::with_schema(0));
        h.observe(1.0);
        h.observe(4.0);
        h.observe(-2.0);
        h.observe(0.0);

        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(4, inner.count);
        assert_eq!(1, native.zero_count);
        assert_eq!(2, native.positive.len());
        assert_eq!(1, native.negative.len());
    }

    #[test]
    fn native_histogram_counts_nan_without_sparse_bucket() {
        let h = Histogram::new_native(NativeHistogramConfig::with_schema(2));
        h.observe(f64::NAN);
        assert_eq!(1, h.count());
        assert!(h.sum().is_nan());

        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(0, native.zero_count);
        assert!(native.positive.is_empty());
        assert!(native.negative.is_empty());
    }

    #[test]
    fn classic_histogram_counts_nan_in_infinity_bucket() {
        let h = Histogram::new([1.0, 2.0]);
        h.observe(f64::NAN);

        let inner = h.inner.lock();
        assert_eq!(1, inner.count);
        assert!(inner.sum.is_nan());
        assert_eq!(1, inner.buckets[2].1);
    }

    #[test]
    fn native_histogram_supports_zero_width_zero_bucket() {
        let h = Histogram::new_native(
            NativeHistogramConfig::with_schema(0)
                .zero_threshold(NATIVE_HISTOGRAM_ZERO_THRESHOLD_ZERO),
        );
        h.observe(0.0);
        h.observe(0.01);

        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(1, native.zero_count);
        assert_eq!(
            1,
            native.positive.iter().map(|(_, count)| *count).sum::<u64>()
        );
    }

    #[test]
    fn native_histogram_zero_threshold_zero_uses_default() {
        let h = Histogram::new_native(NativeHistogramConfig::with_schema(0).zero_threshold(0.0));
        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(
            DEFAULT_NATIVE_HISTOGRAM_ZERO_THRESHOLD,
            native.zero_threshold
        );
    }

    #[test]
    fn native_histogram_negative_zero_threshold_uses_zero_width_zero_bucket() {
        let h = Histogram::new_native(NativeHistogramConfig::with_schema(0).zero_threshold(-2.0));
        h.observe(0.0);
        h.observe(0.01);

        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(NATIVE_HISTOGRAM_ZERO_THRESHOLD_ZERO, native.zero_threshold);
        assert_eq!(1, native.zero_count);
        assert_eq!(
            1,
            native.positive.iter().map(|(_, count)| *count).sum::<u64>()
        );
    }

    #[test]
    fn native_histogram_reduces_resolution_when_max_bucket_limit_is_hit() {
        let h = Histogram::new_native(NativeHistogramConfig::with_schema(8).max_buckets(1));
        h.observe(1.0);
        h.observe(1.1);

        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert!(native.schema < 8);
    }

    #[test]
    fn native_histogram_widens_zero_bucket_before_reducing_resolution() {
        let h = Histogram::new_native(
            NativeHistogramConfig::with_schema(8)
                .max_buckets(1)
                .max_zero_threshold(1.0),
        );
        h.observe(2f64.powi(-100));
        h.observe(1.0);

        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(8, native.schema);
        assert_eq!(1, native.zero_count);
        assert_eq!(1, native.positive.len());
    }

    #[test]
    fn native_histogram_schedules_reset_after_degradation() {
        let h = Histogram::new_native(
            NativeHistogramConfig::with_schema(8)
                .max_buckets(1)
                .min_reset_duration(Duration::from_secs(60)),
        );
        h.observe(1.0);
        h.observe(1.1);

        {
            let mut inner = h.inner.lock();
            let native = inner.native.as_mut().unwrap();
            assert!(native.schema < 8);
            assert!(native.scheduled_reset.is_some());
            native.scheduled_reset = Some(SystemTime::now() - Duration::from_secs(1));
        }

        let snapshot = h.snapshot().unwrap();
        let native = snapshot.native.unwrap();

        assert_eq!(0, snapshot.count);
        assert_eq!(8, native.schema);
        assert_eq!(0, native.zero_count);
        assert!(native.positive.spans.is_empty());
        assert!(native.negative.spans.is_empty());
    }

    #[test]
    fn native_histogram_scheduled_reset_is_triggered_by_observe() {
        let h = Histogram::new_native(
            NativeHistogramConfig::with_schema(8)
                .max_buckets(1)
                .min_reset_duration(Duration::from_secs(60)),
        );
        h.observe(1.0);
        h.observe(1.1);

        {
            let mut inner = h.inner.lock();
            let native = inner.native.as_mut().unwrap();
            assert!(native.schema < 8);
            assert!(native.scheduled_reset.is_some());
            native.scheduled_reset = Some(SystemTime::now() - Duration::from_secs(1));
        }

        h.observe(2.0);

        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(1, inner.count);
        assert_eq!(2.0, inner.sum);
        assert_eq!(8, native.schema);
        assert_eq!(0, native.zero_count);
        assert_eq!(1, native.positive.len());
        assert_eq!(1, native.positive[0].1);
        assert!(native.negative.is_empty());
    }

    #[test]
    fn native_histogram_resets_when_limit_is_hit_after_min_reset_duration() {
        let h = Histogram::new_native(
            NativeHistogramConfig::with_schema(8)
                .max_buckets(1)
                .min_reset_duration(Duration::from_secs(1)),
        );
        h.observe(1.0);
        {
            let mut inner = h.inner.lock();
            inner.native.as_mut().unwrap().created = SystemTime::now() - Duration::from_secs(2);
        }
        h.observe(2.0);

        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(1, inner.count);
        assert_eq!(2.0, inner.sum);
        assert_eq!(8, native.schema);
        assert_eq!(
            1,
            native.positive.iter().map(|(_, count)| *count).sum::<u64>()
        );
    }

    #[test]
    fn native_histogram_zero_min_reset_duration_does_not_reset() {
        let h = Histogram::new_native(
            NativeHistogramConfig::with_schema(8)
                .max_buckets(1)
                .min_reset_duration(Duration::ZERO),
        );
        h.observe(1.0);
        h.observe(1.1);

        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(2, inner.count);
        assert!(native.schema < 8);
        assert!(native.scheduled_reset.is_none());
    }

    #[test]
    fn native_histogram_supports_bucket_factor_constructor() {
        let h = Histogram::new_native(NativeHistogramConfig::new(1.1));
        let inner = h.inner.lock();
        assert_eq!(3, inner.native.as_ref().unwrap().schema);
    }

    #[test]
    fn native_histogram_default_uses_recommended_bucket_factor() {
        let h = Histogram::new_native(NativeHistogramConfig::default());
        let inner = h.inner.lock();
        assert_eq!(3, inner.native.as_ref().unwrap().schema);
    }

    // Copying tests from Go https://github.com/prometheus/client_golang/blob/ef80579ce2dff4ae60710045fd1d38e366adaed0/prometheus/histogram_test.go#L472
    // 1:1 to check compatibility
    #[derive(Debug)]
    struct NativeHistogramParityCase {
        name: &'static str,
        observations: &'static [f64],
        bucket_factor: f64,
        zero_threshold: Option<f64>,
        max_buckets: usize,
        max_zero_threshold: f64,
        sample_count: u64,
        sample_sum: f64,
        schema: i32,
        expected_zero_threshold: f64,
        zero_count: u64,
        negative_spans: &'static [(i32, u32)],
        negative_deltas: &'static [i64],
        positive_spans: &'static [(i32, u32)],
        positive_deltas: &'static [i64],
    }

    #[test]
    fn native_histogram_matches_client_golang_scenarios() {
        const DEFAULT_ZERO_THRESHOLD: f64 = 2.938735877055719e-39;
        const ZERO_WIDTH_ZERO_BUCKET: f64 = NATIVE_HISTOGRAM_ZERO_THRESHOLD_ZERO;
        const NAN: f64 = f64::NAN;
        const INF: f64 = f64::INFINITY;
        const NEG_INF: f64 = f64::NEG_INFINITY;

        let cases = [
            NativeHistogramParityCase {
                name: "no observations",
                observations: &[],
                bucket_factor: 1.1,
                zero_threshold: None,
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 0,
                sample_sum: 0.0,
                schema: 3,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 0,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[],
                positive_deltas: &[],
            },
            NativeHistogramParityCase {
                name: "no observations and zero threshold of zero resulting in no-op span",
                observations: &[],
                bucket_factor: 1.1,
                zero_threshold: Some(ZERO_WIDTH_ZERO_BUCKET),
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 0,
                sample_sum: 0.0,
                schema: 3,
                expected_zero_threshold: 0.0,
                zero_count: 0,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(0, 0)],
                positive_deltas: &[],
            },
            NativeHistogramParityCase {
                name: "factor 1.1 results in schema 3",
                observations: &[0.0, 1.0, 2.0, 3.0],
                bucket_factor: 1.1,
                zero_threshold: None,
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 4,
                sample_sum: 6.0,
                schema: 3,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(0, 1), (7, 1), (4, 1)],
                positive_deltas: &[1, 0, 0],
            },
            NativeHistogramParityCase {
                name: "factor 1.2 results in schema 2",
                observations: &[0.0, 1.0, 1.2, 1.4, 1.8, 2.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 6,
                sample_sum: 7.4,
                schema: 2,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(0, 5)],
                positive_deltas: &[1, -1, 2, -2, 2],
            },
            NativeHistogramParityCase {
                name: "factor 4 results in schema -1",
                observations: &[
                    0.0156251, 0.0625, 0.1, 0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 3.5, 5.0, 6.0, 7.0,
                    33.33,
                ],
                bucket_factor: 4.0,
                zero_threshold: None,
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 14,
                sample_sum: 63.2581251,
                schema: -1,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 0,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(-2, 6)],
                positive_deltas: &[2, 0, 0, 2, -1, -2],
            },
            NativeHistogramParityCase {
                name: "factor 17 results in schema -2",
                observations: &[
                    0.0156251, 0.0625, 0.1, 0.25, 0.5, 1.0, 1.5, 2.0, 3.0, 3.5, 5.0, 6.0, 7.0,
                    33.33,
                ],
                bucket_factor: 17.0,
                zero_threshold: None,
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 14,
                sample_sum: 63.2581251,
                schema: -2,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 0,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(-1, 4)],
                positive_deltas: &[2, 2, 3, -6],
            },
            NativeHistogramParityCase {
                name: "negative buckets",
                observations: &[0.0, -1.0, -1.2, -1.4, -1.8, -2.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 6,
                sample_sum: -7.4,
                schema: 2,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[(0, 5)],
                negative_deltas: &[1, -1, 2, -2, 2],
                positive_spans: &[],
                positive_deltas: &[],
            },
            NativeHistogramParityCase {
                name: "negative and positive buckets",
                observations: &[0.0, -1.0, -1.2, -1.4, -1.8, -2.0, 1.0, 1.2, 1.4, 1.8, 2.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 11,
                sample_sum: 0.0,
                schema: 2,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[(0, 5)],
                negative_deltas: &[1, -1, 2, -2, 2],
                positive_spans: &[(0, 5)],
                positive_deltas: &[1, -1, 2, -2, 2],
            },
            NativeHistogramParityCase {
                name: "wide zero bucket",
                observations: &[0.0, -1.0, -1.2, -1.4, -1.8, -2.0, 1.0, 1.2, 1.4, 1.8, 2.0],
                bucket_factor: 1.2,
                zero_threshold: Some(1.4),
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 11,
                sample_sum: 0.0,
                schema: 2,
                expected_zero_threshold: 1.4,
                zero_count: 7,
                negative_spans: &[(4, 1)],
                negative_deltas: &[2],
                positive_spans: &[(4, 1)],
                positive_deltas: &[2],
            },
            NativeHistogramParityCase {
                name: "NaN observation",
                observations: &[0.0, 1.0, 1.2, 1.4, 1.8, 2.0, NAN],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 7,
                sample_sum: NAN,
                schema: 2,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(0, 5)],
                positive_deltas: &[1, -1, 2, -2, 2],
            },
            NativeHistogramParityCase {
                name: "+Inf observation",
                observations: &[0.0, 1.0, 1.2, 1.4, 1.8, 2.0, INF],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 7,
                sample_sum: INF,
                schema: 2,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(0, 5), (4092, 1)],
                positive_deltas: &[1, -1, 2, -2, 2, -1],
            },
            NativeHistogramParityCase {
                name: "-Inf observation",
                observations: &[0.0, 1.0, 1.2, 1.4, 1.8, 2.0, NEG_INF],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 0,
                max_zero_threshold: 0.0,
                sample_count: 7,
                sample_sum: NEG_INF,
                schema: 2,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[(4097, 1)],
                negative_deltas: &[1],
                positive_spans: &[(0, 5)],
                positive_deltas: &[1, -1, 2, -2, 2],
            },
            NativeHistogramParityCase {
                name: "limited buckets but nothing triggered",
                observations: &[0.0, 1.0, 1.2, 1.4, 1.8, 2.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 4,
                max_zero_threshold: 0.0,
                sample_count: 6,
                sample_sum: 7.4,
                schema: 2,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(0, 5)],
                positive_deltas: &[1, -1, 2, -2, 2],
            },
            NativeHistogramParityCase {
                name: "buckets limited by halving resolution",
                observations: &[0.0, 1.0, 1.1, 1.2, 1.4, 1.8, 2.0, 3.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 4,
                max_zero_threshold: 0.0,
                sample_count: 8,
                sample_sum: 11.5,
                schema: 1,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(0, 5)],
                positive_deltas: &[1, 2, -1, -2, 1],
            },
            NativeHistogramParityCase {
                name: "buckets limited by widening the zero bucket",
                observations: &[0.0, 1.0, 1.1, 1.2, 1.4, 1.8, 2.0, 3.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 4,
                max_zero_threshold: 1.2,
                sample_count: 8,
                sample_sum: 11.5,
                schema: 2,
                expected_zero_threshold: 1.0,
                zero_count: 2,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(1, 7)],
                positive_deltas: &[1, 1, -2, 2, -2, 0, 1],
            },
            NativeHistogramParityCase {
                name: "buckets limited by widening the zero bucket twice",
                observations: &[0.0, 1.0, 1.1, 1.2, 1.4, 1.8, 2.0, 3.0, 4.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 4,
                max_zero_threshold: 1.2,
                sample_count: 9,
                sample_sum: 15.5,
                schema: 2,
                expected_zero_threshold: 1.189207115002721,
                zero_count: 3,
                negative_spans: &[],
                negative_deltas: &[],
                positive_spans: &[(2, 7)],
                positive_deltas: &[2, -2, 2, -2, 0, 1, 0],
            },
            NativeHistogramParityCase {
                name: "limited buckets but nothing triggered, negative observations",
                observations: &[0.0, -1.0, -1.2, -1.4, -1.8, -2.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 4,
                max_zero_threshold: 0.0,
                sample_count: 6,
                sample_sum: -7.4,
                schema: 2,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[(0, 5)],
                negative_deltas: &[1, -1, 2, -2, 2],
                positive_spans: &[],
                positive_deltas: &[],
            },
            NativeHistogramParityCase {
                name: "buckets limited by halving resolution, negative observations",
                observations: &[0.0, -1.0, -1.1, -1.2, -1.4, -1.8, -2.0, -3.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 4,
                max_zero_threshold: 0.0,
                sample_count: 8,
                sample_sum: -11.5,
                schema: 1,
                expected_zero_threshold: DEFAULT_ZERO_THRESHOLD,
                zero_count: 1,
                negative_spans: &[(0, 5)],
                negative_deltas: &[1, 2, -1, -2, 1],
                positive_spans: &[],
                positive_deltas: &[],
            },
            NativeHistogramParityCase {
                name: "buckets limited by widening the zero bucket, negative observations",
                observations: &[0.0, -1.0, -1.1, -1.2, -1.4, -1.8, -2.0, -3.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 4,
                max_zero_threshold: 1.2,
                sample_count: 8,
                sample_sum: -11.5,
                schema: 2,
                expected_zero_threshold: 1.0,
                zero_count: 2,
                negative_spans: &[(1, 7)],
                negative_deltas: &[1, 1, -2, 2, -2, 0, 1],
                positive_spans: &[],
                positive_deltas: &[],
            },
            NativeHistogramParityCase {
                name: "buckets limited by widening the zero bucket twice, negative observations",
                observations: &[0.0, -1.0, -1.1, -1.2, -1.4, -1.8, -2.0, -3.0, -4.0],
                bucket_factor: 1.2,
                zero_threshold: None,
                max_buckets: 4,
                max_zero_threshold: 1.2,
                sample_count: 9,
                sample_sum: -15.5,
                schema: 2,
                expected_zero_threshold: 1.189207115002721,
                zero_count: 3,
                negative_spans: &[(2, 7)],
                negative_deltas: &[2, -2, 2, -2, 0, 1, 0],
                positive_spans: &[],
                positive_deltas: &[],
            },
        ];

        for case in cases {
            let mut config = NativeHistogramConfig::new(case.bucket_factor)
                .max_buckets(case.max_buckets)
                .max_zero_threshold(case.max_zero_threshold);
            if let Some(zero_threshold) = case.zero_threshold {
                config = config.zero_threshold(zero_threshold);
            }
            let h = Histogram::new_native(config);
            for observation in case.observations {
                h.observe(*observation);
            }

            let snapshot = h.snapshot().unwrap();
            let native = snapshot.native.unwrap();

            assert_eq!(case.sample_count, snapshot.count, "{}", case.name);
            if case.sample_sum.is_nan() {
                assert!(snapshot.sum.is_nan(), "{}", case.name);
            } else {
                assert_eq!(case.sample_sum, snapshot.sum, "{}", case.name);
            }
            assert_eq!(case.schema, native.schema, "{}", case.name);
            assert_eq!(
                case.expected_zero_threshold, native.zero_threshold,
                "{}",
                case.name
            );
            assert_eq!(case.zero_count, native.zero_count, "{}", case.name);
            assert_eq!(case.negative_spans, native.negative.spans, "{}", case.name);
            assert_eq!(
                case.negative_deltas, native.negative.deltas,
                "{}",
                case.name
            );
            assert_eq!(case.positive_spans, native.positive.spans, "{}", case.name);
            assert_eq!(
                case.positive_deltas, native.positive.deltas,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn native_histogram_maps_positive_infinity_into_sparse_bucket() {
        let h = Histogram::new_native(NativeHistogramConfig::with_schema(4));
        h.observe(f64::INFINITY);
        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(
            Some(1),
            native
                .positive
                .iter()
                .find(|(index, _)| *index == 16385)
                .map(|(_, count)| *count)
        );
        assert!(!native.positive.iter().any(|(index, _)| *index == i32::MAX));
    }

    #[test]
    fn native_histogram_widens_zero_bucket_at_min_schema_when_limited() {
        let mut inner = NativeHistogramState {
            initial_zero_threshold: 0.0,
            initial_schema: SCHEMA_MIN,
            zero_threshold: 0.0,
            zero_count: 0,
            schema: SCHEMA_MIN,
            max_buckets: 1,
            min_reset_duration: None,
            max_zero_threshold: 1.0,
            created: SystemTime::now(),
            scheduled_reset: None,
            positive: vec![(-10, 2), (0, 1)],
            negative: Vec::new(),
        };

        assert!(widen_zero_bucket(&mut inner));
        assert!(inner.zero_threshold > 0.0);
        assert!(inner.zero_count >= 2);
    }

    #[test]
    fn native_histogram_encodes_spans_and_deltas() {
        let buckets = vec![(-10, 1), (-7, 7), (-5, 7), (2, 8)];

        let encoding = encode_spans_and_deltas(&buckets).unwrap();

        assert_eq!(encoding.spans, vec![(-10, 6), (6, 1)]);
        assert_eq!(encoding.deltas, vec![1, -1, 0, 7, -7, 7, 1]);
    }

    #[test]
    fn native_histogram_downsamples_like_go_client() {
        let buckets = vec![(-2, 1), (-1, 2), (0, 4), (1, 8)];

        let downsampled = downsample_buckets(&buckets);

        assert_eq!(downsampled, vec![(-1, 1), (0, 6), (1, 8)]);
    }

    #[test]
    fn native_histogram_positive_upper_bound_uses_bucket_index() {
        assert_eq!(1.0, positive_upper_bound(0, 0));
        assert_eq!(2.0, positive_upper_bound(0, 1));
        assert!((positive_upper_bound(3, 8) - 2.0).abs() <= 4.0 * f64::EPSILON);
        assert_eq!(f64::MAX, positive_upper_bound(0, 1024));
        assert_eq!(f64::INFINITY, positive_upper_bound(0, 1025));
        assert_eq!(f64::MAX, positive_upper_bound(8, 262144));
        assert_eq!(f64::INFINITY, positive_upper_bound(8, 262145));
    }

    #[test]
    fn native_histogram_bucket_index_matches_standard_boundaries() {
        assert_eq!(0, bucket_index(0, 1.0, false));
        assert_eq!(
            1,
            bucket_index(0, f64::from_bits(1.0f64.to_bits() + 1), false)
        );
        assert_eq!(8, bucket_index(3, 2.0, false));
        assert_eq!(
            -1074,
            bucket_index(0, f64::MIN_POSITIVE / 2f64.powi(52), false)
        );
        assert_eq!(1024, bucket_index(0, f64::MAX, false));
        assert_eq!(1025, bucket_index(0, f64::INFINITY, true));
        assert_eq!(262144, bucket_index(8, f64::MAX, false));
        assert_eq!(262145, bucket_index(8, f64::INFINITY, true));
    }

    #[test]
    fn native_histogram_bounds_match_standard_formula() {
        for schema in 0..=8 {
            let bounds = NATIVE_HISTOGRAM_BOUNDS[schema];
            assert_eq!(1 << schema, bounds.len());

            for (i, bound) in bounds.iter().enumerate() {
                let expected = 2f64.powf(i as f64 / (1 << schema) as f64) / 2.0;
                assert!(
                    (*bound - expected).abs() <= 4.0 * f64::EPSILON,
                    "schema {schema} index {i}: expected {expected}, got {bound}"
                );
            }
        }
    }

    #[test]
    fn native_histogram_no_op_span_marks_zero_width_nan_only_histogram() {
        let h = Histogram::new_native(
            NativeHistogramConfig::with_schema(0)
                .zero_threshold(NATIVE_HISTOGRAM_ZERO_THRESHOLD_ZERO),
        );
        h.observe(f64::NAN);

        let snapshot = h.snapshot().unwrap();
        let native = snapshot.native.unwrap();

        assert_eq!(1, snapshot.count);
        assert_eq!(0.0, native.zero_threshold);
        assert_eq!(0, native.zero_count);
        assert_eq!(vec![(0, 0)], native.positive.spans);
        assert!(native.positive.deltas.is_empty());
        assert!(native.negative.spans.is_empty());
        assert!(native.negative.deltas.is_empty());
    }

    #[test]
    fn native_histogram_fails_for_counts_not_fitting_delta_wire_type() {
        let buckets = vec![(0, i64::MAX as u64 + 1)];

        assert!(encode_spans_and_deltas(&buckets).is_err());
    }

    #[test]
    fn classic_and_native_histogram_updates_both_representations() {
        let h =
            Histogram::new_classic_and_native([1.0, 2.0], NativeHistogramConfig::with_schema(0));
        h.observe(1.0);
        h.observe(4.0);

        let inner = h.inner.lock();
        let native = inner.native.as_ref().unwrap();
        assert_eq!(2, inner.count);
        assert_eq!(5.0, inner.sum);
        assert_eq!(1, inner.buckets[0].1);
        assert_eq!(1, inner.buckets[2].1);
        assert_eq!(
            2,
            native.positive.iter().map(|(_, count)| *count).sum::<u64>()
        );
    }
}
