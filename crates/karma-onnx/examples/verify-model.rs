use std::{env, process::ExitCode};

use karma_onnx::VerifiedImageModel;

fn main() -> ExitCode {
    let Some(manifest_path) = env::args_os().nth(1) else {
        eprintln!("status=unavailable component=image_inference error=manifest_invalid");
        return ExitCode::FAILURE;
    };
    match VerifiedImageModel::load(manifest_path) {
        Ok(model) => {
            println!(
                "status=verified version={} bytes={}",
                model.manifest().asset.version,
                model.manifest().file_bytes
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!(
                "status=unavailable component=image_inference error={}",
                error.kind()
            );
            ExitCode::FAILURE
        }
    }
}
