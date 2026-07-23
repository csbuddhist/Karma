use karma_ai::{
    BgraFrame, FrameDimensions, FrameMetadata, FramePipeline, FrameScheduler, FrameWork,
};
use karma_domain::MonitorId;

fn frame(monitor: &str, captured_at_ms: i64, value: u8) -> BgraFrame {
    BgraFrame::new(
        MonitorId(monitor.into()),
        captured_at_ms,
        FrameDimensions::new(1, 1).unwrap(),
        4,
        vec![value, value, value, 255],
    )
    .unwrap()
}

#[test]
fn first_prepared_frame_requests_image_and_ocr() {
    let output = FramePipeline::default()
        .process(frame("m", 1_000, 10))
        .unwrap();
    assert!(output.work.run_image);
    assert!(output.work.run_ocr);
    assert_eq!(
        output.frame.dimensions(),
        FrameDimensions::new(1, 1).unwrap()
    );
}

#[test]
fn unchanged_frame_respects_scheduler_limits() {
    let mut pipeline = FramePipeline::default();
    pipeline.process(frame("m", 1_000, 10)).unwrap();
    let output = pipeline.process(frame("m", 2_000, 10)).unwrap();
    assert!(output.work.run_image);
    assert!(!output.work.run_ocr);
}

#[test]
fn backwards_timestamp_does_not_run_or_overflow() {
    let mut scheduler = FrameScheduler::default();
    scheduler.select(FrameMetadata {
        monitor_id: MonitorId("m".into()),
        captured_at_ms: i64::MAX,
        fingerprint: 1,
    });
    let output = scheduler.select(FrameMetadata {
        monitor_id: MonitorId("m".into()),
        captured_at_ms: i64::MIN,
        fingerprint: 2,
    });
    assert_eq!(
        output,
        FrameWork {
            run_image: false,
            run_ocr: false,
        }
    );
}
