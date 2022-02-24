use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(&["src/encoding/proto/openmetrics_data_model.proto"], &["src/encoding/proto/"])?;
    Ok(())
}
