#[cfg(any(windows, test))]
mod network_enforcement;
#[cfg(windows)]
mod scm;
#[cfg(windows)]
mod scm_update;
#[cfg(any(windows, test))]
mod update_transaction;
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
