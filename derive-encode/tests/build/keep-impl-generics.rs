use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::encoding::EncodeLabelValue;

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelValue)]
enum CpuUsageLabelMode {
    User,
    System,
    Irq,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
struct CpuUsageLabelSet<'a> { // <-- `'a` lifetime is used in the struct,
                              // this should be preserved in the impl block
    mode: CpuUsageLabelMode,
    service: &'a str
}

fn main() {}
