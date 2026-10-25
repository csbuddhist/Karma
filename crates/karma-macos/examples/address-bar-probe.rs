//! Prints the host each running browser's address bar shows, to check the
//! Accessibility reader on real browsers. Run from a terminal that has the
//! Accessibility permission:
//! `cargo run -p karma-macos --features session-agent --example address-bar-probe -- PID...`

#[cfg(target_os = "macos")]
fn main() {
    println!(
        "accessibility_trusted={}",
        karma_macos::accessibility::is_trusted()
    );
    for pid in std::env::args()
        .skip(1)
        .filter_map(|pid| pid.parse::<u32>().ok())
    {
        let started = std::time::Instant::now();
        let host = karma_macos::accessibility::browser_host(pid);
        println!(
            "pid={pid} host={} elapsed_ms={}",
            host.as_deref().unwrap_or("-"),
            started.elapsed().as_millis()
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}
