use std::fmt;

use karma_domain::MonitorId;
use zeroize::Zeroizing;

use crate::{FrameDimensions, FrameRegion, ImageClassifier, ImageInference, PreparedFrame};

/// Edge of a square motion cell, in prepared-frame pixels.
const CELL_EDGE: usize = 8;
/// Mean-luma difference at which a cell counts as changed between two samples.
const CHANGE_THRESHOLD: u8 = 4;
/// A cell is moving when it changed in at least this many of its last 16 samples.
const MOTION_CHANGES: u32 = 2;
/// Narrower or shorter motion is mostly avatars, emoji and icons, and too blurry to classify
/// once enlarged to the model input.
const MINIMUM_MOTION_EDGE: u32 = 56;
/// Larger motion is already legible in the whole-frame inference.
const MAXIMUM_MOTION_AREA_PERCENT: u64 = 50;
/// Moving cells must fill this share of their bounds, which rejects cursor trails and sparse UI.
const MINIMUM_MOTION_FILL_PERCENT: usize = 50;
/// Longer, thinner motion is a ticker or a text line rather than video.
const MAXIMUM_MOTION_ASPECT_RATIO: u32 = 4;
/// Bounds the extra inferences that one frame can cost.
const MAXIMUM_FOCUS_REGIONS: usize = 2;

/// Finds the parts of a monitor that keep changing, such as a video embedded in a web page or a
/// small player window, so they can be classified at their own scale.
#[derive(Default)]
pub struct MotionRegionTracker {
    state: Option<MotionState>,
}

impl fmt::Debug for MotionRegionTracker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MotionRegionTracker")
            .field(
                "grid",
                &self.state.as_ref().map(|state| (state.columns, state.rows)),
            )
            .finish()
    }
}

struct MotionState {
    monitor_id: MonitorId,
    dimensions: FrameDimensions,
    columns: usize,
    rows: usize,
    lumas: Zeroizing<Vec<u8>>,
    /// One bit per sample, newest in the lowest bit, set when the cell changed.
    history: Vec<u16>,
}

impl MotionRegionTracker {
    /// Records `frame` and returns its densest moving regions, largest first.
    ///
    /// A monitor or resolution change restarts the history, so the first frame of a new
    /// sequence has no regions.
    pub fn observe(&mut self, frame: &PreparedFrame) -> Vec<FrameRegion> {
        let dimensions = frame.dimensions();
        let columns = (dimensions.width() as usize).div_ceil(CELL_EDGE);
        let rows = (dimensions.height() as usize).div_ceil(CELL_EDGE);
        let lumas = cell_lumas(frame, columns, rows);
        match &mut self.state {
            Some(state)
                if state.monitor_id == *frame.monitor_id() && state.dimensions == dimensions =>
            {
                for ((history, previous), current) in state
                    .history
                    .iter_mut()
                    .zip(state.lumas.iter())
                    .zip(lumas.iter())
                {
                    let changed = previous.abs_diff(*current) >= CHANGE_THRESHOLD;
                    *history = (*history << 1) | u16::from(changed);
                }
                state.lumas = lumas;
                state.regions()
            }
            _ => {
                self.state = Some(MotionState {
                    monitor_id: frame.monitor_id().clone(),
                    dimensions,
                    columns,
                    rows,
                    lumas,
                    history: vec![0; columns * rows],
                });
                Vec::new()
            }
        }
    }
}

fn cell_lumas(frame: &PreparedFrame, columns: usize, rows: usize) -> Zeroizing<Vec<u8>> {
    let dimensions = frame.dimensions();
    let width = dimensions.width() as usize;
    let height = dimensions.height() as usize;
    let mut sums = Zeroizing::new(vec![0u32; columns * rows]);
    for (y, row) in frame.pixels().chunks_exact(width * 4).enumerate() {
        let cells = &mut sums[(y / CELL_EDGE) * columns..][..columns];
        for (x, pixel) in row.chunks_exact(4).enumerate() {
            cells[x / CELL_EDGE] +=
                (77 * u32::from(pixel[2]) + 150 * u32::from(pixel[1]) + 29 * u32::from(pixel[0]))
                    >> 8;
        }
    }
    let mut lumas = Zeroizing::new(Vec::with_capacity(columns * rows));
    for (index, sum) in sums.iter().enumerate() {
        let cell_width = CELL_EDGE.min(width - (index % columns) * CELL_EDGE);
        let cell_height = CELL_EDGE.min(height - (index / columns) * CELL_EDGE);
        lumas.push((*sum / (cell_width * cell_height) as u32) as u8);
    }
    lumas
}

impl MotionState {
    fn regions(&self) -> Vec<FrameRegion> {
        let moving = self
            .history
            .iter()
            .map(|history| history.count_ones() >= MOTION_CHANGES)
            .collect::<Vec<_>>();
        // Growing every moving cell by one joins the still patches inside a video into one
        // region and leaves a one-cell margin around it.
        let grown = self.grow(&moving);
        let mut visited = vec![false; grown.len()];
        let mut candidates = Vec::new();
        let mut pending = Vec::new();
        for start in 0..grown.len() {
            if !grown[start] || visited[start] {
                continue;
            }
            visited[start] = true;
            pending.push(start);
            let mut crop = CellBounds::at(start % self.columns, start / self.columns);
            let mut motion: Option<CellBounds> = None;
            let mut moving_cells = 0;
            while let Some(index) = pending.pop() {
                let (column, row) = (index % self.columns, index / self.columns);
                crop.include(column, row);
                if moving[index] {
                    moving_cells += 1;
                    motion
                        .get_or_insert(CellBounds::at(column, row))
                        .include(column, row);
                }
                let neighbors = [
                    (column > 0).then(|| index - 1),
                    (column + 1 < self.columns).then(|| index + 1),
                    (row > 0).then(|| index - self.columns),
                    (row + 1 < self.rows).then(|| index + self.columns),
                ];
                for neighbor in neighbors.into_iter().flatten() {
                    if grown[neighbor] && !visited[neighbor] {
                        visited[neighbor] = true;
                        pending.push(neighbor);
                    }
                }
            }
            if let Some(region) =
                motion.and_then(|motion| self.focus_region(crop, motion, moving_cells))
            {
                candidates.push((moving_cells, region));
            }
        }
        candidates.sort_by(|first, second| second.0.cmp(&first.0));
        candidates
            .into_iter()
            .take(MAXIMUM_FOCUS_REGIONS)
            .map(|(_, region)| region)
            .collect()
    }

    fn grow(&self, cells: &[bool]) -> Vec<bool> {
        let mut grown = vec![false; cells.len()];
        for (index, _) in cells.iter().enumerate().filter(|(_, moving)| **moving) {
            let (column, row) = (index % self.columns, index / self.columns);
            for neighbor_row in row.saturating_sub(1)..=(row + 1).min(self.rows - 1) {
                for neighbor_column in column.saturating_sub(1)..=(column + 1).min(self.columns - 1)
                {
                    grown[neighbor_row * self.columns + neighbor_column] = true;
                }
            }
        }
        grown
    }

    /// Judges the moving cells of one group and returns the area to crop around them.
    fn focus_region(
        &self,
        crop: CellBounds,
        motion: CellBounds,
        moving_cells: usize,
    ) -> Option<FrameRegion> {
        if moving_cells * 100 < motion.cells() * MINIMUM_MOTION_FILL_PERCENT {
            return None;
        }
        let (_, _, width, height) = self.pixels(motion);
        let short_edge = width.min(height);
        if short_edge < MINIMUM_MOTION_EDGE
            || width.max(height) > short_edge * MAXIMUM_MOTION_ASPECT_RATIO
            || u64::from(width) * u64::from(height) * 100
                > u64::from(self.dimensions.width())
                    * u64::from(self.dimensions.height())
                    * MAXIMUM_MOTION_AREA_PERCENT
        {
            return None;
        }
        let (x, y, width, height) = self.pixels(crop);
        FrameRegion::new(x, y, width, height).ok()
    }

    /// Converts cell bounds to an `(x, y, width, height)` pixel rectangle inside the frame.
    fn pixels(&self, bounds: CellBounds) -> (u32, u32, u32, u32) {
        let x = bounds.left * CELL_EDGE;
        let y = bounds.top * CELL_EDGE;
        let right = ((bounds.right + 1) * CELL_EDGE).min(self.dimensions.width() as usize);
        let bottom = ((bounds.bottom + 1) * CELL_EDGE).min(self.dimensions.height() as usize);
        (x as u32, y as u32, (right - x) as u32, (bottom - y) as u32)
    }
}

/// Inclusive cell bounds of a connected group of cells.
#[derive(Clone, Copy)]
struct CellBounds {
    left: usize,
    top: usize,
    right: usize,
    bottom: usize,
}

impl CellBounds {
    fn at(column: usize, row: usize) -> Self {
        Self {
            left: column,
            top: row,
            right: column,
            bottom: row,
        }
    }

    fn include(&mut self, column: usize, row: usize) {
        self.left = self.left.min(column);
        self.top = self.top.min(row);
        self.right = self.right.max(column);
        self.bottom = self.bottom.max(row);
    }

    fn cells(self) -> usize {
        (self.right - self.left + 1) * (self.bottom - self.top + 1)
    }
}

/// Classifies the whole frame and then each moving region on its own, keeping the riskiest
/// result.
///
/// The whole-frame inference shrinks a video that covers a quarter of the screen to a small patch
/// of the model input among unrelated page content, which dilutes its score. Classifying the crop
/// gives the model the video at its own scale.
pub struct MotionFocusClassifier<C> {
    inner: C,
    motion: MotionRegionTracker,
}

impl<C> MotionFocusClassifier<C> {
    pub fn new(inner: C) -> Self {
        Self {
            inner,
            motion: MotionRegionTracker::default(),
        }
    }
}

impl<C: ImageClassifier> ImageClassifier for MotionFocusClassifier<C> {
    type Error = C::Error;

    fn classify(&mut self, frame: &PreparedFrame) -> Result<ImageInference, Self::Error> {
        let regions = self.motion.observe(frame);
        let mut riskiest = self.inner.classify(frame)?;
        for region in regions {
            // A focus crop only adds evidence: when it cannot be classified, the whole-frame
            // result still stands.
            let Ok(focus) = frame.crop(region) else {
                continue;
            };
            match self.inner.classify(&focus) {
                Ok(inference) if inference.score_millis > riskiest.score_millis => {
                    riskiest = inference;
                }
                Ok(_) | Err(_) => {}
            }
        }
        Ok(riskiest)
    }
}

#[cfg(test)]
mod tests {
    use karma_domain::{MonitorId, RiskCategory};

    use super::*;
    use crate::{BgraFrame, FramePreparationConfig, FramePreparer};

    const WIDTH: u32 = 640;
    const HEIGHT: u32 = 360;
    const BACKGROUND: u8 = 128;

    struct Patch {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        value: u8,
    }

    fn patch(x: u32, y: u32, width: u32, height: u32, value: u8) -> Patch {
        Patch {
            x,
            y,
            width,
            height,
            value,
        }
    }

    fn sized_frame(monitor: &str, width: u32, height: u32, patches: &[Patch]) -> PreparedFrame {
        let dimensions = FrameDimensions::new(width, height).unwrap();
        let mut pixels = vec![BACKGROUND; dimensions.tight_byte_len().unwrap()];
        for patch in patches {
            for y in patch.y..patch.y + patch.height {
                for x in patch.x..patch.x + patch.width {
                    let offset = ((y * width + x) * 4) as usize;
                    pixels[offset..offset + 4].copy_from_slice(&[
                        patch.value,
                        patch.value,
                        255,
                        255,
                    ]);
                }
            }
        }
        FramePreparer::new(FramePreparationConfig::new(width.max(height)).unwrap())
            .prepare(
                BgraFrame::new(
                    MonitorId(monitor.into()),
                    1,
                    dimensions,
                    dimensions.tight_stride().unwrap(),
                    pixels,
                )
                .unwrap(),
            )
            .unwrap()
    }

    fn frame(patches: &[Patch]) -> PreparedFrame {
        sized_frame("display-1", WIDTH, HEIGHT, patches)
    }

    /// Feeds three samples in which every rectangle alternates between two shades.
    fn observe_moving(
        tracker: &mut MotionRegionTracker,
        rectangles: &[(u32, u32, u32, u32)],
    ) -> Vec<FrameRegion> {
        let mut regions = Vec::new();
        for value in [40, 200, 40] {
            let patches = rectangles
                .iter()
                .map(|&(x, y, width, height)| patch(x, y, width, height, value))
                .collect::<Vec<_>>();
            regions = tracker.observe(&frame(&patches));
        }
        regions
    }

    fn contains(region: FrameRegion, x: u32, y: u32, width: u32, height: u32) -> bool {
        let dimensions = region.dimensions();
        region.x() <= x
            && region.y() <= y
            && region.x() + dimensions.width() >= x + width
            && region.y() + dimensions.height() >= y + height
    }

    #[test]
    fn video_inside_a_static_page_becomes_a_focus_region_with_a_small_margin() {
        let mut tracker = MotionRegionTracker::default();

        let regions = observe_moving(&mut tracker, &[(320, 160, 160, 120)]);

        assert_eq!(regions.len(), 1);
        assert!(contains(regions[0], 320, 160, 160, 120));
        assert!(contains(
            FrameRegion::new(312, 152, 176, 136).unwrap(),
            regions[0].x(),
            regions[0].y(),
            regions[0].dimensions().width(),
            regions[0].dimensions().height(),
        ));
    }

    #[test]
    fn content_that_changed_only_once_is_not_motion() {
        let mut tracker = MotionRegionTracker::default();
        tracker.observe(&frame(&[]));

        assert!(
            tracker
                .observe(&frame(&[patch(320, 160, 160, 120, 40)]))
                .is_empty()
        );
        assert!(
            tracker
                .observe(&frame(&[patch(320, 160, 160, 120, 40)]))
                .is_empty()
        );
    }

    #[test]
    fn motion_stays_focused_while_a_video_shows_a_still_scene() {
        let mut tracker = MotionRegionTracker::default();
        observe_moving(&mut tracker, &[(320, 160, 160, 120)]);

        let regions = tracker.observe(&frame(&[patch(320, 160, 160, 120, 40)]));

        assert_eq!(regions.len(), 1);
    }

    fn fresh_moving_regions(rectangles: &[(u32, u32, u32, u32)]) -> Vec<FrameRegion> {
        observe_moving(&mut MotionRegionTracker::default(), rectangles)
    }

    #[test]
    fn full_screen_motion_is_left_to_the_whole_frame_inference() {
        assert!(fresh_moving_regions(&[(0, 0, WIDTH, HEIGHT)]).is_empty());
        assert!(fresh_moving_regions(&[(0, 0, 480, 300)]).is_empty());
    }

    #[test]
    fn small_player_window_is_still_focused() {
        assert_eq!(fresh_moving_regions(&[(500, 280, 107, 60)]).len(), 1);
    }

    #[test]
    fn small_animations_and_thin_tickers_are_ignored() {
        assert!(fresh_moving_regions(&[(100, 100, 40, 40)]).is_empty());
        assert!(fresh_moving_regions(&[(100, 100, 120, 44)]).is_empty());
        assert!(fresh_moving_regions(&[(40, 200, 480, 64)]).is_empty());
    }

    #[test]
    fn sparse_motion_such_as_a_cursor_trail_is_ignored() {
        let trail = (0..10)
            .map(|step| (100 + step * 24, 60 + step * 24, 8, 8))
            .collect::<Vec<_>>();

        assert!(fresh_moving_regions(&trail).is_empty());
    }

    #[test]
    fn regions_are_ranked_by_size_and_capped() {
        let mut tracker = MotionRegionTracker::default();

        let regions = observe_moving(
            &mut tracker,
            &[(16, 16, 96, 96), (200, 16, 200, 140), (440, 200, 128, 128)],
        );

        assert_eq!(regions.len(), MAXIMUM_FOCUS_REGIONS);
        assert!(contains(regions[0], 200, 16, 200, 140));
        assert!(contains(regions[1], 440, 200, 128, 128));
    }

    #[test]
    fn monitor_or_resolution_change_restarts_the_history() {
        let mut tracker = MotionRegionTracker::default();
        observe_moving(&mut tracker, &[(320, 160, 160, 120)]);

        let moved = patch(320, 160, 160, 120, 200);
        assert!(
            tracker
                .observe(&sized_frame("display-2", WIDTH, HEIGHT, &[moved]))
                .is_empty()
        );
        let moved = patch(320, 160, 160, 120, 40);
        assert!(
            tracker
                .observe(&sized_frame("display-2", WIDTH, 400, &[moved]))
                .is_empty()
        );
    }

    #[test]
    fn tracker_debug_does_not_expose_screen_content() {
        let mut tracker = MotionRegionTracker::default();
        tracker.observe(&frame(&[patch(0, 0, 8, 8, 77)]));

        let debug = format!("{tracker:?}");

        assert_eq!(debug, "MotionRegionTracker { grid: Some((80, 45)) }");
    }

    /// Scores a frame by the share of its pixels with a full red channel, like a model that
    /// recognizes content only when it fills enough of the input.
    #[derive(Default)]
    struct RedShareClassifier {
        calls: usize,
        failing_call: Option<usize>,
    }

    impl ImageClassifier for RedShareClassifier {
        type Error = ();

        fn classify(&mut self, frame: &PreparedFrame) -> Result<ImageInference, Self::Error> {
            self.calls += 1;
            if self.failing_call == Some(self.calls) {
                return Err(());
            }
            let pixels = frame.pixels().chunks_exact(4);
            let total = pixels.len();
            let red = pixels.filter(|pixel| pixel[0] != BACKGROUND).count();
            Ok(ImageInference {
                score_millis: (red * 1_000 / total) as u16,
                categories: vec![RiskCategory::Nudity],
            })
        }
    }

    fn classify_video_frames(
        classifier: &mut MotionFocusClassifier<RedShareClassifier>,
    ) -> Vec<u16> {
        [40, 200, 40]
            .into_iter()
            .map(|value| {
                classifier
                    .classify(&frame(&[patch(320, 160, 160, 120, value)]))
                    .unwrap()
                    .score_millis
            })
            .collect()
    }

    #[test]
    fn embedded_video_is_scored_at_its_own_scale() {
        let mut classifier = MotionFocusClassifier::new(RedShareClassifier::default());

        let scores = classify_video_frames(&mut classifier);

        assert_eq!(&scores[..2], &[83, 83]);
        assert!(scores[2] >= 750, "focused score {}", scores[2]);
        assert_eq!(classifier.inner.calls, 4);
    }

    #[test]
    fn static_screen_runs_only_the_whole_frame_inference() {
        let mut classifier = MotionFocusClassifier::new(RedShareClassifier::default());

        for _ in 0..4 {
            classifier
                .classify(&frame(&[patch(320, 160, 160, 120, 40)]))
                .unwrap();
        }

        assert_eq!(classifier.inner.calls, 4);
    }

    #[test]
    fn failed_focus_inference_keeps_the_whole_frame_result() {
        let mut classifier = MotionFocusClassifier::new(RedShareClassifier {
            failing_call: Some(4),
            ..Default::default()
        });

        let scores = classify_video_frames(&mut classifier);

        assert_eq!(scores, vec![83, 83, 83]);
    }

    #[test]
    fn failed_whole_frame_inference_is_reported() {
        let mut classifier = MotionFocusClassifier::new(RedShareClassifier {
            failing_call: Some(1),
            ..Default::default()
        });

        assert_eq!(classifier.classify(&frame(&[])).err(), Some(()));
    }
}
