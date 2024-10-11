use procfs::process::Process;
use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::counter::ConstCounter,
    registry::Unit,
};

#[derive(Debug)]
pub struct ProcessCollector {
    namespace: String,
}

impl Collector for ProcessCollector {
    fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        let tps = procfs::ticks_per_second();
        // process_cpu_seconds_total Total user and system CPU time spent in seconds.
        // process_max_fds Maximum number of open file descriptors.
        // process_open_fds Number of open file descriptors.
        // process_virtual_memory_bytes Virtual memory size in bytes.
        // process_resident_memory_bytes Resident memory size in bytes.
        // process_virtual_memory_max_bytes Maximum amount of virtual memory available in bytes.
        // process_start_time_seconds Start time of the process since unix epoch in seconds.
        // process_network_receive_bytes_total Number of bytes received by the process over the network.
        // process_network_transmit_bytes_total Number of bytes sent by the process over the network.

        if let Ok(proc) = Process::myself() {
            if let Ok(stat) = proc.stat() {
                let cpu_time = (stat.stime + stat.utime) / tps as u64;
                let counter = ConstCounter::new(cpu_time);
                let metric_encoder = encoder.encode_descriptor(
                    "process_cpu_seconds_total",
                    "Total user and system CPU time spent in seconds.",
                    Some(&Unit::Seconds),
                    counter.metric_type(),
                )?;
                counter.encode(metric_encoder)?;
            }

            if let Ok(limits) = proc.limits() {
                let max_fds = match limits.max_open_files.soft_limit {
                    procfs::process::LimitValue::Value(v) => v,
                    procfs::process::LimitValue::Unlimited => 0,
                };
                let counter = ConstCounter::new(max_fds);
                let metric_encoder = encoder.encode_descriptor(
                    "process_max_fds",
                    "Maximum number of open file descriptors.",
                    None,
                    counter.metric_type(),
                )?;
                counter.encode(metric_encoder)?;
            }
        }

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
