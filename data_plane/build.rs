fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "grpc")]
    {
        // Compile protobuf files for gRPC services
        if let Err(e) = tonic_build::configure()
            .build_server(true)
            .build_client(true) // 启用客户端代码生成
            .compile(
                &[
                    "../proto/experiment.proto",  // 数据面服务
                    "../proto/config_push.proto", // Configuration push service
                ],
                &["../proto/"],
            )
        {
            eprintln!("Warning: Failed to compile protobuf: {}", e);
            eprintln!("To enable gRPC support, install protoc:");
            eprintln!("  macOS: brew install protobuf");
            eprintln!("  Ubuntu: apt install protobuf-compiler");
            eprintln!("  Or download from: https://github.com/protocolbuffers/protobuf/releases");
            return Err(Box::new(e));
        }

        println!("cargo:rerun-if-changed=../proto/experiment.proto");
        println!("cargo:rerun-if-changed=../proto/config_push.proto");
    }
    Ok(())
}
