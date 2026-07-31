#[cfg(windows)]
mod windows_service;

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    windows_service::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("KarmaService can only run on Windows");
}
