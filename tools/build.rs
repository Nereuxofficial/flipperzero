use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const FLIPPER_PROTO: &str = "flipperzero-protobuf/flipper.proto";

fn main() -> io::Result<()> {
    // - We don't include the proto files in the repository so that downstreams do not
    //   need to regenerate the bindings even if protoc is present.
    // - We check for the existence of protoc in the same way as prost-build, so that
    //   people building from source do not need to have protoc installed.
    if Path::new(FLIPPER_PROTO).exists()
        && env::var_os("PROTOC")
            .map(PathBuf::from)
            .or_else(|| which::which("protoc").ok())
            .is_some()
    {
        gen_protobufs()?;
    }

    Ok(())
}

fn gen_protobufs() -> io::Result<()> {
    let out: PathBuf = env::var_os("OUT_DIR")
        .expect("Cannot find OUT_DIR environment variable")
        .into();

    // Build the compact format types.
    prost_build::compile_protos(
        &[
            "flipperzero-protobuf/application.proto",
            "flipperzero-protobuf/desktop.proto",
            "flipperzero-protobuf/flipper.proto",
            "flipperzero-protobuf/gpio.proto",
            "flipperzero-protobuf/gui.proto",
            "flipperzero-protobuf/property.proto",
            "flipperzero-protobuf/storage.proto",
            "flipperzero-protobuf/system.proto",
        ],
        &["flipperzero-protobuf/"],
    )?;

    // Copy the generated types into the source tree so changes can be committed.
    fs::copy(out.join("pb.rs"), "src/proto/pb.rs")?;
    fs::copy(out.join("pb_app.rs"), "src/proto/pb_app.rs")?;
    fs::copy(out.join("pb_desktop.rs"), "src/proto/pb_desktop.rs")?;
    fs::copy(out.join("pb_gpio.rs"), "src/proto/pb_gpio.rs")?;
    fs::copy(out.join("pb_gui.rs"), "src/proto/pb_gui.rs")?;
    fs::copy(out.join("pb_property.rs"), "src/proto/pb_property.rs")?;
    fs::copy(out.join("pb_storage.rs"), "src/proto/pb_storage.rs")?;
    fs::copy(out.join("pb_system.rs"), "src/proto/pb_system.rs")?;

    Ok(())
}
