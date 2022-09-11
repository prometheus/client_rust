use std::io::Result;

fn main() -> Result<()> {
    #[cfg(feature = "protobuf")]
    prost_build::compile_protos(
        &["src/encoding/proto/openmetrics_data_model.proto"],
        &["src/encoding/proto/"],
    )?;

    Ok(())
}
