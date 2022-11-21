//! Module implementing an Open Metrics histogram.
//!
//! See [`Summary`] for details.

use super::{MetricType, TypedMetric};
//use owning_ref::OwningRef;
//use std::iter::{self, once};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use quantiles::ckms::CKMS;

/// Open Metrics [`Summary`] to measure distributions of discrete events.
#[derive(Debug)]
pub struct Summary {
    target_quantile: Vec<f64>,
    target_error: f64,
    max_age_buckets: u64,
    max_age_seconds: u64,
    stream_duration: Duration,
    inner: Arc<Mutex<InnerSummary>>,
}

impl Clone for Summary {
    fn clone(&self) -> Self {
        Summary {
            target_quantile: self.target_quantile.clone(),
            target_error: self.target_error,
            max_age_buckets: self.max_age_buckets,
            max_age_seconds: self.max_age_seconds,
            stream_duration: self.stream_duration,
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct InnerSummary {
    sum: f64,
    count: u64,
    quantile_streams: Vec<CKMS<f64>>,
    // head_stream is like a cursor which carries the index
    // of the stream in the quantile_streams that we want to query.
    head_stream_idx: u64,
    // timestamp at which the head_stream_idx was last rotated.
    last_rotated_timestamp: Instant,
}

impl Summary {
    /// Create a new [`Summary`].
    pub fn new(max_age_buckets: u64, max_age_seconds: u64, target_quantile: Vec<f64>, target_error: f64) -> Self {
        let mut streams: Vec<CKMS<f64>> = Vec::new();
        for _ in 0..max_age_buckets {
            streams.push(CKMS::new(target_error));
        }

        let stream_duration = Duration::from_secs(max_age_seconds / max_age_buckets);
        let last_rotated_timestamp = Instant::now();

        if target_quantile.iter().any(|&x| x > 1.0 || x < 0.0) {
            panic!("Quantile value out of range");
        }

        Summary{
            max_age_buckets,
            max_age_seconds,
            stream_duration,
            target_quantile,
            target_error,
            inner: Arc::new(Mutex::new(InnerSummary {
                sum: Default::default(),
                count: Default::default(),
                quantile_streams: streams,
                head_stream_idx: 0,
                last_rotated_timestamp,
            }))
        }
    }

    /// Observe the given value.
    pub fn observe(&self, v: f64) {
        self.rotate_buckets();

        let mut inner = self.inner.lock().unwrap();
        inner.sum += v;
        inner.count += 1;

        // insert quantiles into all streams/buckets.
        for stream in inner.quantile_streams.iter_mut() {
            stream.insert(v);
        }
    }

    /// Retrieve the values of the summary metric.
    pub fn get(&self) -> (f64, u64, Vec<(f64, f64)>) {
        self.rotate_buckets();

        let inner = self.inner.lock().unwrap();
        let sum = inner.sum;
        let count = inner.count;
        let mut quantile_values: Vec<(f64, f64)> = Vec::new();

        for q in self.target_quantile.iter() {
            match inner.quantile_streams[inner.head_stream_idx as usize].query(*q) {
                Some((_, v)) => quantile_values.push((*q, v)),
                None => continue,
            };
        }
        (sum, count, quantile_values)
    }

    fn rotate_buckets(&self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.last_rotated_timestamp.elapsed() >= self.stream_duration {
            inner.last_rotated_timestamp = Instant::now();            
            if inner.head_stream_idx == self.max_age_buckets {
                inner.head_stream_idx = 0;
            } else {
                inner.head_stream_idx += 1;
            }
        };
    }
}

impl TypedMetric for Summary {
    const TYPE: MetricType = MetricType::Summary;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary() {
        let summary = Summary::new(5, 10, vec![0.5, 0.9, 0.99], 0.01);
        summary.observe(1.0);
        summary.observe(5.0);
        summary.observe(10.0);

        let (s, c, q) = summary.get();
        assert_eq!(16.0, s);
        assert_eq!(3, c);
        assert_eq!(vec![(0.5, 5.0), (0.9, 10.0), (0.99, 10.0)], q);
    }

    #[test]
    #[should_panic(expected="Quantile value out of range")]
    fn summary_panic() {
        Summary::new(5, 10, vec![1.0, 5.0, 9.0], 0.01);
    }
}
