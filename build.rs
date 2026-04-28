use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    #[cfg(any(feature = "protobuf", feature = "legacy_protobuf"))]
    compile_protos()?;

    Ok(())
}

#[allow(clippy::vec_init_then_push)] // False positive due to feature flags
#[cfg(any(feature = "protobuf", feature = "legacy_protobuf"))]
fn compile_protos() -> Result<(), Box<dyn Error>> {
    let mut protos = Vec::new();

    #[cfg(feature = "protobuf")]
    protos.push("src/encoding/proto/metrics.proto");
    #[cfg(feature = "legacy_protobuf")]
    protos.push("src/encoding/proto/openmetrics_data_model.proto");

    let includes = ["src/encoding/proto/"];

    #[cfg(feature = "protobuf-protox")]
    prost_build::compile_fds(protox::compile(&protos, includes)?)?;

    #[cfg(not(feature = "protobuf-protox"))]
    prost_build::compile_protos(&protos, &includes)?;

    for path in &protos {
        println!("cargo:rerun-if-changed={}", path);
    }
    for path in &includes {
        println!("cargo:rerun-if-changed={}", path);
    }

    Ok(())
}
