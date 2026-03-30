use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(feature = "protobuf")]
    compile_protos()?;

    Ok(())
}

#[cfg(feature = "protobuf")]
fn compile_protos() -> Result<(), Box<dyn Error>> {
    let protos = ["src/encoding/proto/openmetrics_data_model.proto"];
    let includes = ["src/encoding/proto/"];

    #[cfg(feature = "protobuf-protox")]
    prost_build::compile_fds(protox::compile(protos, includes)?)?;

    #[cfg(not(feature = "protobuf-protox"))]
    prost_build::compile_protos(&protos, &includes)?;

    Ok(())
}
