use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::encoding::EncodeLabelValue;

mod A {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
    struct Unnamed(String);

    #[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
    struct Unit;

    #[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
    enum Enum {
        A,
        B,
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
    enum DataEnum {
        A,
        B(String),
    }

    #[derive(Clone, Copy, EncodeLabelSet)]
    #[repr(C)]
    union Union {
        a: u32,
        b: u64,
    }
}

mod B {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelValue)]
    struct Struct {
        a: String,
        b: String,
    }

    #[derive(Clone, Copy, EncodeLabelValue)]
    #[repr(C)]
    union Union {
        a: u32,
        b: u64,
    }
}

fn main() {}
