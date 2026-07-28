use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::encoding::EncodeLabelValue;

// Unnamed struct fields: not supported by `EncodeLabelSet`.
#[derive(EncodeLabelSet)]
struct Unnamed(String);

// Unit struct: not supported by `EncodeLabelSet`.
#[derive(EncodeLabelSet)]
struct UnitStruct;

// Enum: not supported by `EncodeLabelSet`.
#[derive(EncodeLabelSet)]
enum NotAllowedSet {
    A,
    B,
}

// Unknown attribute: only `#[prometheus(flatten)]` is accepted.
#[derive(EncodeLabelSet)]
struct BadAttr {
    #[prometheus(unknown)]
    a: u64,
}

// Struct: not supported by `EncodeLabelValue` (only enums are).
#[derive(EncodeLabelValue)]
struct StructForValue;

// Union: not supported by either derive.
#[derive(EncodeLabelSet)]
union UnionSet {
    a: u32,
    b: f32,
}

fn main() {}
