use std::time::{SystemTime, UNIX_EPOCH};

use procfs::process::{LimitValue, Process, Stat};
use prometheus_client::{
    collector::Collector,
    encoding::EncodeMetric,
    metrics::{counter::ConstCounter, gauge::ConstGauge},
    registry::Unit,
};

use super::netstat::Netstat;

#[derive(Debug)]
pub struct System {
    namespace: String,
    page_size: u64,
    report_error: bool,
}

impl System {
    pub fn load(namespace: Option<String>, report_error: bool) -> Self {
        let page_size = procfs::page_size();
        let namespace = match namespace {
            Some(mut n) => {
                n.push('_');
                n
            }
            None => "".to_string(),
        };

        Self {
            page_size,
            namespace,
            report_error,
        }
    }

    fn start_time(
        &self,
        encoder: &mut prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
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

        Ok(())
    }

    fn open_fds(
        &self,
        proc: &Process,
        encoder: &mut prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
        let open_file_descriptors = proc.fd_count().map_err(|_| std::fmt::Error)?;
        let counter = ConstCounter::new(open_file_descriptors as u32);
        let metric_name = format!("{}process_open_fds", &self.namespace);
        let metric_encoder = encoder.encode_descriptor(
            &metric_name,
            "Number of open file descriptors.",
            None,
            counter.metric_type(),
        )?;
        counter.encode(metric_encoder)?;

        Ok(())
    }

    fn cpu_seconds_total(
        &self,
        stat: &Stat,
        encoder: &mut prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
        let tps = procfs::ticks_per_second();
        let cpu_time = (stat.stime + stat.utime) / tps;
        let counter = ConstCounter::new(cpu_time);
        let metric_name = format!("{}process_cpu", &self.namespace);
        let metric_encoder = encoder.encode_descriptor(
            &metric_name,
            "Total user and system CPU time spent in seconds.",
            Some(&Unit::Seconds),
            counter.metric_type(),
        )?;
        counter.encode(metric_encoder)?;

        Ok(())
    }

    fn max_fds(
        &self,
        proc: &Process,
        encoder: &mut prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
        // TODO: handle error
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
            let metric_name = format!("{}process_max_fds", &self.namespace);
            let metric_encoder = encoder.encode_descriptor(
                &metric_name,
                "Maximum number of open file descriptors.",
                None,
                gauge.metric_type(),
            )?;
            gauge.encode(metric_encoder)?;
        }

        Ok(())
    }

    fn virtual_memory_max_bytes(
        &self,
        proc: &Process,
        encoder: &mut prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
        if let Ok(limits) = proc.limits() {
            let max_address_space = limits.max_address_space;
            let max_virtual_memory = match max_address_space.soft_limit {
                LimitValue::Unlimited => match max_address_space.hard_limit {
                    LimitValue::Unlimited => 0,
                    LimitValue::Value(hard) => hard,
                },
                LimitValue::Value(soft) => soft,
            };
            let gauge = ConstGauge::new(max_virtual_memory as i64);
            let metric_name = format!("{}process_virtual_memory_max", &self.namespace);
            let metric_encoder = encoder.encode_descriptor(
                &metric_name,
                "Maximum amount of virtual memory available in bytes.",
                None,
                gauge.metric_type(),
            )?;
            gauge.encode(metric_encoder)?;
        }

        Ok(())
    }

    fn virtual_memory_bytes(
        &self,
        stat: &Stat,
        encoder: &mut prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
        let vm_bytes = ConstGauge::new(stat.vsize as i64);
        let metric_name = format!("{}process_virtual_memory", &self.namespace);
        let vme = encoder.encode_descriptor(
            &metric_name,
            "Virtual memory size in bytes",
            Some(&Unit::Bytes),
            vm_bytes.metric_type(),
        )?;
        vm_bytes.encode(vme)?;

        Ok(())
    }

    fn resident_memory_bytes(
        &self,
        stat: &Stat,
        encoder: &mut prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
        let rss_bytes = ConstGauge::new((stat.rss * self.page_size) as i64);
        let metric_name = format!("{}process_resident_memory", &self.namespace);
        let rsse = encoder.encode_descriptor(
            &metric_name,
            "Resident memory size in bytes.",
            Some(&Unit::Bytes),
            rss_bytes.metric_type(),
        )?;
        rss_bytes.encode(rsse)?;

        Ok(())
    }

    fn network_in_out(
        &self,
        stat: &Stat,
        encoder: &mut prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
        let Netstat { ip_ext, .. } = Netstat::read(stat.pid).map_err(|_| std::fmt::Error)?;
        let recv_bytes = ConstCounter::new(ip_ext.in_octets.unwrap_or_default());
        let metric_name = format!("{}process_network_receive", &self.namespace);
        let rbe = encoder.encode_descriptor(
            &metric_name,
            "Number of bytes received by the process over the network.",
            Some(&Unit::Bytes),
            recv_bytes.metric_type(),
        )?;
        recv_bytes.encode(rbe)?;

        let transmit_bytes = ConstCounter::new(ip_ext.out_octets.unwrap_or_default());
        let metric_name = format!("{}process_network_transmit", &self.namespace);
        let tbe = encoder.encode_descriptor(
            &metric_name,
            "Number of bytes sent by the process over the network.",
            Some(&Unit::Bytes),
            transmit_bytes.metric_type(),
        )?;
        transmit_bytes.encode(tbe)?;

        Ok(())
    }

    fn handle_error_report(
        &self,
        result: Result<(), std::fmt::Error>,
    ) -> Result<(), std::fmt::Error> {
        if !self.report_error {
            return Ok(());
        }

        result
    }
}

impl Collector for System {
    fn encode(
        &self,
        mut encoder: prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
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

        self.handle_error_report(self.start_time(&mut encoder))?;
        self.handle_error_report(self.resident_memory_bytes(&stat, &mut encoder))?;
        self.handle_error_report(self.virtual_memory_bytes(&stat, &mut encoder))?;
        self.handle_error_report(self.virtual_memory_max_bytes(&proc, &mut encoder))?;
        self.handle_error_report(self.open_fds(&proc, &mut encoder))?;
        self.handle_error_report(self.max_fds(&proc, &mut encoder))?;
        self.handle_error_report(self.cpu_seconds_total(&stat, &mut encoder))?;
        self.handle_error_report(self.network_in_out(&stat, &mut encoder))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignore_error_report() {
        let system = System::load(None, false);
        let result = system.handle_error_report(Err(std::fmt::Error));
        assert!(result.is_ok(), "handle_error_report did not ignore error");
    }

    #[test]
    fn return_error() {
        let system = System::load(None, true);
        let result = system.handle_error_report(Err(std::fmt::Error));
        assert!(result.is_err(), "handle_error_report ignored error");
    }
}
