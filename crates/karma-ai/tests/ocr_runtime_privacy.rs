use karma_ai::{DetectionMap, DetectionMapError, OcrTextBatch, WordPack, WordRule};
use karma_domain::OcrRisk;
use zeroize::Zeroizing;

#[test]
fn detection_map_is_redacted_and_exposes_only_threshold_decisions() {
    let map = DetectionMap::from_values(2, 2, vec![0.1, 0.3, 0.8, 0.9]).unwrap();

    assert_eq!(map.dimensions(), [2, 2]);
    assert_eq!(format!("{map:?}"), "DetectionMap { width: 2, height: 2 }");
    assert!(!format!("{map:?}").contains("0.8"));

    let mask = map.threshold(0.3).unwrap();
    assert_eq!(mask.is_active(0, 0), Some(false));
    assert_eq!(mask.is_active(1, 0), Some(true));
    assert_eq!(mask.is_active(0, 1), Some(true));
    assert_eq!(mask.is_active(1, 1), Some(true));
    assert_eq!(mask.is_active(2, 0), None);
}

#[test]
fn detection_map_accepts_zeroizing_session_storage_without_unwrapping_it() {
    let values = Zeroizing::new(vec![0.2, 0.8]);

    let map = DetectionMap::from_zeroizing_values(2, 1, values).unwrap();

    assert_eq!(map.dimensions(), [2, 1]);
}

#[test]
fn detection_map_rejects_invalid_storage_and_non_finite_values() {
    assert_eq!(
        DetectionMap::from_values(2, 2, vec![0.0; 3]).unwrap_err(),
        DetectionMapError::InvalidElementCount
    );
    assert_eq!(
        DetectionMap::from_values(1, 1, vec![f32::NAN]).unwrap_err(),
        DetectionMapError::NonFiniteValue
    );
    assert_eq!(
        DetectionMap::from_values(1, 1, vec![f32::INFINITY]).unwrap_err(),
        DetectionMapError::NonFiniteValue
    );
    assert_eq!(
        DetectionMap::from_values(1, 1, vec![1.1]).unwrap_err(),
        DetectionMapError::OutOfRangeValue
    );
}

#[test]
fn detection_map_compares_region_mean_without_returning_a_probability() {
    let map =
        DetectionMap::from_values(3, 3, vec![0.9, 0.9, 0.0, 0.9, 0.3, 0.0, 0.0, 0.0, 0.0]).unwrap();
    let polygon = [[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]];

    assert_eq!(map.region_mean_meets(&polygon, 0.7), Ok(true));
    assert_eq!(map.region_mean_meets(&polygon, 0.8), Ok(false));
}

#[test]
fn word_pack_classifies_a_private_text_batch_without_public_line_refs() {
    let pack = WordPack::compile(vec![WordRule::literal(
        "fixture_category",
        "fixture match",
        OcrRisk::Keyword,
    )])
    .unwrap();
    let batch = OcrTextBatch::from_lines(vec!["fixture match".into()], 64).unwrap();

    let summary = pack.classify_batch(&batch);

    assert_eq!(summary.risk, OcrRisk::Keyword);
    assert_eq!(summary.categories, vec!["fixture_category"]);
}
