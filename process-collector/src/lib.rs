use std::time::{Instant, SystemTime, UNIX_EPOCH};

use procfs::process::{LimitValue, Process};
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::{counter::ConstCounter, gauge::ConstGauge},
    registry::Unit,
};

#[derive(Debug)]
pub struct ProcessCollector {
    namespace: String,
}

impl Collector for ProcessCollector {
    fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        let tps = procfs::ticks_per_second();

        // TODO: handle errors
        let proc = match Process::myself() {
            Ok(proc) => proc,
            Err(_) => {
                return Ok(());
            }
        };
        let stat = match proc.stat() {
            Ok(stat) => stat,
            Err(_) => {
                return Ok(());
            }
        };

        let cpu_time = (stat.stime + stat.utime) / tps;
        let counter = ConstCounter::new(cpu_time);
        let metric_encoder = encoder.encode_descriptor(
            "process_cpu_seconds_total",
            "Total user and system CPU time spent in seconds.",
            Some(&Unit::Seconds),
            counter.metric_type(),
        )?;
        counter.encode(metric_encoder)?;

        if let Ok(limits) = proc.limits() {
            let max_open_files = limits.max_open_files;
            let max_fds = match max_open_files.soft_limit {
                LimitValue::Unlimited => match max_open_files.hard_limit {
                    LimitValue::Unlimited => 0,
                    LimitValue::Value(hard) => hard,
                },
                LimitValue::Value(soft) => soft,
            };
            let gauge = ConstGauge::new(max_fds as i64);
            let metric_encoder = encoder.encode_descriptor(
                "process_max_fds",
                "Maximum number of open file descriptors.",
                None,
                gauge.metric_type(),
            )?;
            gauge.encode(metric_encoder)?;

            let max_address_space = limits.max_address_space;
            let max_virtual_memory = match max_address_space.soft_limit {
                LimitValue::Unlimited => match max_address_space.hard_limit {
                    LimitValue::Unlimited => 0,
                    LimitValue::Value(hard) => hard,
                },
                LimitValue::Value(soft) => soft,
            };
            let gauge = ConstGauge::new(max_fds as i64);
            let metric_encoder = encoder.encode_descriptor(
                "process_virtual_memory_max_bytes",
                "Maximum amount of virtual memory available in bytes.",
                None,
                gauge.metric_type(),
            )?;
            gauge.encode(metric_encoder)?;
        }

        let vm_bytes = ConstGauge::new(stat.vsize as i64);
        let vme = encoder.encode_descriptor(
            "process_virtual_memory_bytes",
            "Virtual memory size in bytes",
            Some(&Unit::Bytes),
            vm_bytes.metric_type(),
        )?;
        vm_bytes.encode(vme)?;

        // TODO: add rss_bytes (fix self.page_size)
        //
        // let rss_bytes = ConstGauge::new((stat.rss * self.page_size) as i64);
        // let rsse = encoder.encode_descriptor(
        //     "process_resident_memory_bytes",
        //     "Resident memory size in bytes.",
        //     Some(&Unit::Bytes),
        //     rss_bytes.metric_type(),
        // )?;
        // rss_bytes.encode(rsse)?;

        let start_time_from_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            // TODO: remove expect
            .expect("process start time");
        let start_time = ConstGauge::new(start_time_from_epoch.as_secs_f64());
        let start_time_metric = encoder.encode_descriptor(
            "process_start_time_seconds",
            "Start time of the process since unix epoch in seconds.",
            Some(&Unit::Seconds),
            start_time.metric_type(),
        )?;
        start_time.encode(start_time_metric)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prometheus_client::registry::Registry;

    #[test]
    fn register_process_collector() {
        let mut registry = Registry::default();
        registry.register_collector(Box::new(ProcessCollector {
            namespace: String::new(),
        }))
    }
}
