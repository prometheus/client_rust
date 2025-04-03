use std::io;

use libc::{self};
use procfs::process::{LimitValue, Process};
use prometheus_client::{
    collector::Collector,
    encoding::EncodeMetric,
    metrics::{counter::ConstCounter, gauge::ConstGauge},
    registry::Unit,
};

#[derive(Debug)]
pub(crate) struct System {
    page_size: u64,
}

impl System {
    pub fn load() -> std::io::Result<Self> {
        let page_size = page_size()?;
        Ok(Self { page_size })
    }
}

impl Collector for System {
    fn encode(
        &self,
        mut encoder: prometheus_client::encoding::DescriptorEncoder,
    ) -> Result<(), std::fmt::Error> {
        let tps = procfs::ticks_per_second();

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
            let gauge = ConstGauge::new(max_virtual_memory as i64);
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

        let rss_bytes = ConstGauge::new((stat.rss * self.page_size) as i64);
        let rsse = encoder.encode_descriptor(
            "process_resident_memory_bytes",
            "Resident memory size in bytes.",
            Some(&Unit::Bytes),
            rss_bytes.metric_type(),
        )?;
        rss_bytes.encode(rsse)?;

        Ok(())
    }
}

fn page_size() -> io::Result<u64> {
    sysconf(libc::_SC_PAGESIZE)
}

#[allow(unsafe_code)]
fn sysconf(num: libc::c_int) -> Result<u64, io::Error> {
    match unsafe { libc::sysconf(num) } {
        e if e <= 0 => {
            let error = io::Error::last_os_error();
            Err(error)
        }
        val => Ok(val as u64),
    }
}
