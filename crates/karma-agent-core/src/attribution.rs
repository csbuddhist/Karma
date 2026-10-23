#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn intersection_area(self, other: Self) -> u64 {
        let width = i64::from(self.right.min(other.right))
            .saturating_sub(i64::from(self.left.max(other.left)))
            .max(0) as u64;
        let height = i64::from(self.bottom.min(other.bottom))
            .saturating_sub(i64::from(self.top.max(other.top)))
            .max(0) as u64;
        width.saturating_mul(height)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowCandidate {
    pub handle: isize,
    pub pid: u32,
    pub bounds: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttributedWindow {
    pub handle: isize,
    pub pid: u32,
    pub overlap_area: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnreliableReason {
    MissingForegroundProcess,
    NoOverlap,
    EqualOverlap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributionResult {
    Reliable(AttributedWindow),
    Unreliable(UnreliableReason),
}

pub struct SourceAttributor;

impl SourceAttributor {
    pub fn resolve(
        monitor: Rect,
        foreground_pid: Option<u32>,
        candidates: &[WindowCandidate],
    ) -> AttributionResult {
        let Some(foreground_pid) = foreground_pid else {
            return AttributionResult::Unreliable(UnreliableReason::MissingForegroundProcess);
        };

        let mut best = None;
        let mut tied = false;
        for candidate in candidates
            .iter()
            .filter(|candidate| candidate.pid == foreground_pid)
        {
            let overlap_area = monitor.intersection_area(candidate.bounds);
            if overlap_area == 0 {
                continue;
            }

            match best {
                None => {
                    best = Some(AttributedWindow {
                        handle: candidate.handle,
                        pid: candidate.pid,
                        overlap_area,
                    });
                    tied = false;
                }
                Some(current) if overlap_area > current.overlap_area => {
                    best = Some(AttributedWindow {
                        handle: candidate.handle,
                        pid: candidate.pid,
                        overlap_area,
                    });
                    tied = false;
                }
                Some(current) if overlap_area == current.overlap_area => tied = true,
                Some(_) => {}
            }
        }

        match (best, tied) {
            (Some(_), true) => AttributionResult::Unreliable(UnreliableReason::EqualOverlap),
            (Some(value), false) => AttributionResult::Reliable(value),
            (None, _) => AttributionResult::Unreliable(UnreliableReason::NoOverlap),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
        Rect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn window(handle: isize, pid: u32, bounds: Rect) -> WindowCandidate {
        WindowCandidate {
            handle,
            pid,
            bounds,
        }
    }

    #[test]
    fn intersection_area_handles_overlap_and_touching_edges() {
        assert_eq!(
            rect(0, 0, 100, 100).intersection_area(rect(50, 20, 120, 80)),
            3_000
        );
        assert_eq!(
            rect(0, 0, 100, 100).intersection_area(rect(100, 0, 120, 80)),
            0
        );
    }

    #[test]
    fn unique_largest_foreground_window_is_reliable() {
        let result = SourceAttributor::resolve(
            rect(0, 0, 100, 100),
            Some(7),
            &[
                window(1, 7, rect(0, 0, 40, 40)),
                window(2, 7, rect(0, 0, 80, 80)),
            ],
        );

        assert_eq!(
            result,
            AttributionResult::Reliable(AttributedWindow {
                handle: 2,
                pid: 7,
                overlap_area: 6_400,
            })
        );
    }

    #[test]
    fn background_process_window_is_ignored() {
        let result = SourceAttributor::resolve(
            rect(0, 0, 100, 100),
            Some(7),
            &[
                window(1, 7, rect(0, 0, 40, 40)),
                window(2, 8, rect(0, 0, 100, 100)),
            ],
        );

        assert_eq!(
            result,
            AttributionResult::Reliable(AttributedWindow {
                handle: 1,
                pid: 7,
                overlap_area: 1_600,
            })
        );
    }

    #[test]
    fn equal_best_overlaps_are_ambiguous() {
        let result = SourceAttributor::resolve(
            rect(0, 0, 100, 100),
            Some(7),
            &[
                window(1, 7, rect(0, 0, 50, 50)),
                window(2, 7, rect(50, 50, 100, 100)),
            ],
        );

        assert_eq!(
            result,
            AttributionResult::Unreliable(UnreliableReason::EqualOverlap)
        );
    }

    #[test]
    fn missing_foreground_and_no_overlap_are_distinct() {
        assert_eq!(
            SourceAttributor::resolve(rect(0, 0, 100, 100), None, &[]),
            AttributionResult::Unreliable(UnreliableReason::MissingForegroundProcess)
        );
        assert_eq!(
            SourceAttributor::resolve(
                rect(0, 0, 100, 100),
                Some(7),
                &[window(1, 7, rect(200, 200, 300, 300))],
            ),
            AttributionResult::Unreliable(UnreliableReason::NoOverlap)
        );
    }
}
