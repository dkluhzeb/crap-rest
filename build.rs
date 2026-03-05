use std::{env, io::Result, path::PathBuf};

fn main() -> Result<()> {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    tonic_prost_build::configure()
        .file_descriptor_set_path(out_dir.join("content_descriptor.bin"))
        .compile_protos(&["proto/content.proto"], &["proto"])?;
    Ok(())
}
