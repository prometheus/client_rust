fn main() {
	  prost_build::compile_protos(&["src/encoding/proto/open_metrics.proto"], &["src/encoding/proto"]).unwrap();
}
