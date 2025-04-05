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
    namespace: String,
    #[cfg(target_os = "linux")]
    system: linux::System,
}

impl ProcessCollector {
    pub fn new(namespace: Option<String>) -> std::io::Result<Self> {
        #[cfg(target_os = "linux")]
        let system = linux::System::load(namespace.clone())?;
        let namespace = match namespace {
            Some(mut n) => {
                n.push('_');
                n
            }
            None => "".to_string(),
        };

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
            .map_err(|_| std::fmt::Error)?;
        let start_time = ConstGauge::new(start_time_from_epoch.as_secs_f64());
        let metric_name = format!("{}process_start_time", self.namespace);
        let start_time_metric = encoder.encode_descriptor(
            &metric_name,
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
    use prometheus_client::{encoding::text::encode, registry::Registry};

    #[test]
    fn register_start_time() {
        let mut registry = Registry::default();
        let processor_collector = ProcessCollector::new(None).unwrap();
        registry.register_collector(Box::new(processor_collector));
        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let start_time = "# HELP process_start_time_seconds Start time of the process since unix epoch in seconds.\n".to_owned() +
        "# TYPE process_start_time_seconds gauge\n" +
        "# UNIT process_start_time_seconds seconds\n" + 
        "process_start_time_seconds ";

        assert!(
            encoded.contains(&start_time),
            "encoded does not contain expected start_time"
        );
    }

    #[test]
    fn register_resident_memory() {
        let mut registry = Registry::default();
        let processor_collector = ProcessCollector::new(None).unwrap();
        registry.register_collector(Box::new(processor_collector));
        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let resident_memory =
            "# HELP process_resident_memory_bytes Resident memory size in bytes.\n".to_owned()
                + "# TYPE process_resident_memory_bytes gauge\n"
                + "# UNIT process_resident_memory_bytes bytes\n"
                + "process_resident_memory_bytes ";

        assert!(
            encoded.contains(&resident_memory),
            "encoded does not contain expected resident_memory"
        );
    }

    #[test]
    fn register_virtual_memory() {
        let mut registry = Registry::default();
        let processor_collector = ProcessCollector::new(None).unwrap();
        registry.register_collector(Box::new(processor_collector));
        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let virtual_memory = "# HELP process_virtual_memory_bytes Virtual memory size in bytes\n"
            .to_owned()
            + "# TYPE process_virtual_memory_bytes gauge\n"
            + "# UNIT process_virtual_memory_bytes bytes\n"
            + "process_virtual_memory_bytes ";

        assert!(
            encoded.contains(&virtual_memory),
            "encoded does not contain expected virtual_memory"
        );
    }

    #[test]
    fn register_virtual_memory_max() {
        let mut registry = Registry::default();
        let processor_collector = ProcessCollector::new(None).unwrap();
        registry.register_collector(Box::new(processor_collector));
        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let virtual_memory_max = "# HELP process_virtual_memory_max Maximum amount of virtual memory available in bytes.\n".to_owned() +
            "# TYPE process_virtual_memory_max gauge\n" +
            "process_virtual_memory_max ";

        assert!(
            encoded.contains(&virtual_memory_max),
            "encoded does not contain expected virtual_memory_max"
        );
    }

    #[test]
    fn register_open_fds() {
        let mut registry = Registry::default();
        let processor_collector = ProcessCollector::new(None).unwrap();
        registry.register_collector(Box::new(processor_collector));
        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let open_fds = "# HELP process_open_fds Number of open file descriptors.\n".to_owned()
            + "# TYPE process_open_fds counter\n"
            + "process_open_fds_total ";

        assert!(
            encoded.contains(&open_fds),
            "encoded does not contain expected open_fds"
        );
    }

    #[test]
    fn register_max_fds() {
        let mut registry = Registry::default();
        let processor_collector = ProcessCollector::new(None).unwrap();
        registry.register_collector(Box::new(processor_collector));
        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let max_fds = "# HELP process_max_fds Maximum number of open file descriptors.\n"
            .to_owned()
            + "# TYPE process_max_fds gauge\n"
            + "process_max_fds ";

        assert!(
            encoded.contains(&max_fds),
            "encoded does not contain expected max_fds"
        );
    }

    #[test]
    fn register_cpu_seconds() {
        let mut registry = Registry::default();
        let processor_collector = ProcessCollector::new(None).unwrap();
        registry.register_collector(Box::new(processor_collector));
        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let cpu_seconds =
            "# HELP process_cpu_seconds Total user and system CPU time spent in seconds.\n"
                .to_owned()
                + "# TYPE process_cpu_seconds counter\n"
                + "# UNIT process_cpu_seconds seconds\n"
                + "process_cpu_seconds_total ";

        assert!(
            encoded.contains(&cpu_seconds),
            "encoded does not contain expected cpu_seconds"
        );
    }

    #[test]
    fn register_network_receive() {
        let mut registry = Registry::default();
        let processor_collector = ProcessCollector::new(None).unwrap();
        registry.register_collector(Box::new(processor_collector));
        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let network_receive = "# HELP process_network_receive_bytes Number of bytes received by the process over the network.\n".to_owned() +
        "# TYPE process_network_receive_bytes counter\n" +
        "# UNIT process_network_receive_bytes bytes\n" +
        "process_network_receive_bytes_total ";

        assert!(
            encoded.contains(&network_receive),
            "encoded does not contain expected network_receive"
        );
    }

    #[test]
    fn register_network_transmit() {
        let mut registry = Registry::default();
        let processor_collector = ProcessCollector::new(None).unwrap();
        registry.register_collector(Box::new(processor_collector));
        let mut encoded = String::new();
        encode(&mut encoded, &registry).unwrap();

        let network_transmit = "# HELP process_network_transmit_bytes Number of bytes sent by the process over the network.\n".to_owned() +
        "# TYPE process_network_transmit_bytes counter\n" +
        "# UNIT process_network_transmit_bytes bytes\n" +
        "process_network_transmit_bytes_total ";

        assert!(
            encoded.contains(&network_transmit),
            "encoded does not contain expected network_transmit"
        );
    }
}
