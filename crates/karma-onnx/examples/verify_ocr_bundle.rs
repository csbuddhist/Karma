use std::{env, path::PathBuf, process::ExitCode};

use karma_ai::OcrModelProfile;
use karma_onnx::{InferenceErrorKind, VerifiedOcrBundle};

fn main() -> ExitCode {
    let mut arguments = env::args_os().skip(1);
    let Some(manifest_path) = arguments.next() else {
        eprintln!("status=unavailable component=ocr error=ocr_contract_invalid");
        return ExitCode::FAILURE;
    };
    if arguments.next().is_some() {
        eprintln!("status=unavailable component=ocr error=ocr_contract_invalid");
        return ExitCode::FAILURE;
    }

    match verify(PathBuf::from(manifest_path)) {
        Ok((profile, version)) => {
            println!("status=verified profile={profile} version={version}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("status=unavailable component=ocr error={error}");
            ExitCode::FAILURE
        }
    }
}

fn verify(manifest_path: PathBuf) -> Result<(&'static str, String), InferenceErrorKind> {
    let bundle = VerifiedOcrBundle::load(manifest_path).map_err(|error| error.kind())?;
    let profile = match bundle.profile() {
        OcrModelProfile::Lightweight => "lightweight",
        OcrModelProfile::Accurate => "accurate",
    };
    let version = bundle.manifest().asset.version.clone();
    let engine = bundle.create_engine().map_err(|error| error.kind())?;
    drop(engine);
    Ok((profile, version))
}
