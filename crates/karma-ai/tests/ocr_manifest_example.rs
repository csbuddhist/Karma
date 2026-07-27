use karma_ai::OcrBundleManifest;

#[test]
fn reviewed_ppocrv5_manifest_example_matches_the_runtime_schema() {
    let manifest: OcrBundleManifest = serde_json::from_str(include_str!(
        "../../../assets/ocr/pp-ocrv5-mobile/manifest.example.json"
    ))
    .expect("manifest example must deserialize through the strict runtime schema");

    manifest
        .validate()
        .expect("manifest example must satisfy the runtime contract");
}
