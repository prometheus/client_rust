use prometheus_client::{
    collector::Collector,
    encoding::{DescriptorEncoder, EncodeMetric},
    metrics::gauge::ConstGauge,
    registry::Unit,
};
use std::time::{SystemTime, UNIX_EPOCH};

mod linux;

#[derive(Debug)]
pub struct ProcessCollector {
    namespace: Option<String>,
    #[cfg(target_os = "linux")]
    system: linux::System,
}

impl ProcessCollector {
    pub fn new(namespace: Option<String>) -> std::io::Result<Self> {
        #[cfg(target_os = "linux")]
        let system = linux::System::load()?;

        Ok(ProcessCollector {
            namespace,
            #[cfg(target_os = "linux")]
            system,
        })
    }
}

impl Collector for ProcessCollector {
    fn encode(&self, mut encoder: DescriptorEncoder) -> Result<(), std::fmt::Error> {
        let start_time_from_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| std::fmt::Error)?;
        let start_time = ConstGauge::new(start_time_from_epoch.as_secs_f64());
        let start_time_metric = encoder.encode_descriptor(
            "process_start_time_seconds",
            "Start time of the process since unix epoch in seconds.",
            Some(&Unit::Seconds),
            start_time.metric_type(),
        )?;
        start_time.encode(start_time_metric)?;

        #[cfg(target_os = "linux")]
        self.system.encode(encoder)?;

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
        // registry.register_collector(Box::new(ProcessCollector::new(None)))
    }
}
