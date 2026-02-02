//! Build script for generating gRPC code from protobuf definitions.
//!
//! This script uses tonic-build to compile the A2A protocol buffer definitions
//! into Rust code when the `grpc` feature is enabled.
//!
//! Google API dependencies are provided by the `google-api-proto` crate.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "grpc")]
    {
        compile_protos()?;
    }

    Ok(())
}

#[cfg(feature = "grpc")]
fn compile_protos() -> Result<(), Box<dyn std::error::Error>> {
    // Proto file paths (now inside ra2a crate)
    let proto_file = "proto/a2a.proto";
    let proto_dir = "proto";
    let googleapis_dir = "proto/googleapis";

    // Check if proto file exists
    if !std::path::Path::new(proto_file).exists() {
        println!("cargo:warning=Proto file not found: {}", proto_file);
        return Ok(());
    }

    // Check if googleapis submodule is initialized
    if !std::path::Path::new(googleapis_dir).exists() {
        println!("cargo:warning=googleapis submodule not found. Run: git submodule update --init");
        return Ok(());
    }

    // Configure and compile protos using tonic_prost_build
    // Google API types are from googleapis submodule, mapped to google-api-proto crate
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(true)
        // Use external types from google-api-proto for Google API dependencies
        .extern_path(".google.api", "::google_api_proto::google::api")
        .compile_protos(
            &[proto_file],
            // Include proto dir and googleapis submodule for imports
            &[proto_dir, googleapis_dir],
        )?;

    // Tell cargo to rerun if the proto file changes
    println!("cargo:rerun-if-changed={}", proto_file);
    println!("cargo:rerun-if-changed={}", proto_dir);

    Ok(())
}
