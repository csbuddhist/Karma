#[cfg(windows)]
use std::io::Read;

#[cfg(windows)]
use karma_ipc::{ClientKind, RequestEnvelope, ServiceRequest, ServiceResult};
#[cfg(windows)]
use karma_windows_ipc::send_request;
#[cfg(windows)]
use rand::{RngCore, rngs::OsRng};
#[cfg(windows)]
use zeroize::Zeroizing;

#[cfg(windows)]
fn main() {
    if std::env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new("shutdown")) {
        eprintln!("usage: KarmaControl shutdown (password is read from standard input)");
        std::process::exit(2);
    }
    let mut password = Zeroizing::new(String::new());
    if std::io::stdin().read_to_string(&mut password).is_err() {
        eprintln!("failed to read administrator password");
        std::process::exit(2);
    }
    while password.ends_with('\r') || password.ends_with('\n') {
        password.pop();
    }
    let request = RequestEnvelope::new(
        opaque(),
        opaque(),
        ClientKind::Installer,
        ServiceRequest::RequestShutdown {
            password: password.to_string(),
        },
    );
    let response = match send_request(&request, 3000) {
        Ok(response) => response,
        Err(_) => {
            eprintln!("KarmaService is not reachable; shutdown was not requested");
            std::process::exit(4);
        }
    };
    match response.result {
        Ok(ServiceResult::Acknowledged) => println!("KarmaService accepted the shutdown request"),
        Ok(_) => {
            eprintln!("unexpected service response; shutdown was not requested");
            std::process::exit(3);
        }
        Err(_) => {
            eprintln!("administrator authentication failed; shutdown was not requested");
            std::process::exit(3);
        }
    }
}

#[cfg(windows)]
fn opaque() -> String {
    use std::fmt::Write;
    let mut bytes = [0_u8; 24];
    OsRng.fill_bytes(&mut bytes);
    bytes
        .iter()
        .fold(String::with_capacity(48), |mut value, byte| {
            let _ = write!(value, "{byte:02x}");
            value
        })
}

#[cfg(not(windows))]
fn main() {
    eprintln!("KarmaControl can only run on Windows");
}
