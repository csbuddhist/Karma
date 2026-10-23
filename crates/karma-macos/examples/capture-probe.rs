//! Checks ScreenCaptureKit capture on this Mac for the acceptance checklist.
//! Prints only counts, sizes, and statuses; never window titles or pixels.

#[cfg(target_os = "macos")]
fn main() {
    use std::time::{Duration, Instant};

    use karma_macos::{capture::DisplayStream, display};

    println!(
        "screen_capture_allowed={}",
        display::screen_capture_allowed()
    );
    let windows = display::front_to_back_windows();
    println!(
        "windows={} titled_windows={}",
        windows.len(),
        windows
            .iter()
            .filter(|window| !window.title.is_empty())
            .count()
    );
    let displays = match display::active_displays() {
        Ok(displays) => displays,
        Err(error) => {
            println!("displays=unavailable error={error}");
            std::process::exit(1);
        }
    };
    println!("displays={}", displays.len());
    for snapshot in &displays {
        let started = Instant::now();
        match DisplayStream::start(snapshot) {
            Ok(mut stream) => {
                let frames = stream.frames();
                let deadline = Instant::now() + Duration::from_secs(3);
                let mut received = 0_u32;
                let mut dimensions = None;
                while Instant::now() < deadline {
                    if let Some(frame) = frames.next_frame(Duration::from_millis(500)) {
                        received += 1;
                        dimensions = Some(frame.dimensions());
                    }
                }
                println!(
                    "display={} start_ms={} frames_in_3s={} size={:?} status={:?}",
                    snapshot.monitor_id(),
                    started.elapsed().as_millis(),
                    received,
                    dimensions.map(|size| (size.width(), size.height())),
                    stream.status()
                );
                stream.stop();
            }
            Err(error) => println!(
                "display={} stream=unavailable error={error}",
                snapshot.monitor_id()
            ),
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}
