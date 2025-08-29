use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Compile endpoint service proto
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .file_descriptor_set_path(out_dir.join("endpoint_descriptor.bin"))
        .compile_protos(&["proto/endpoint_service.proto"], &["proto"])
        .unwrap_or_else(|e| panic!("Failed to compile endpoint proto files: {}", e));

    // Compile sentence service proto
    tonic_build::configure()
        .protoc_arg("--experimental_allow_proto3_optional")
        .file_descriptor_set_path(out_dir.join("sentence_descriptor.bin"))
        .compile_protos(&["proto/sentence_service.proto"], &["proto"])
        .unwrap_or_else(|e| panic!("Failed to compile sentence proto files: {}", e));
}
