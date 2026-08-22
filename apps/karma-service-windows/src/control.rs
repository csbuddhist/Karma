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
    match request(
        ClientKind::Installer,
        ServiceRequest::RequestShutdown {
            password: password.to_string(),
        },
    ) {
        Ok(ServiceResult::Acknowledged) => println!("KarmaService accepted the shutdown request"),
        _ => {
            eprintln!("administrator authentication failed; shutdown was not requested");
            std::process::exit(3);
        }
    }
}

#[cfg(windows)]
fn request(client: ClientKind, body: ServiceRequest) -> Result<ServiceResult, ()> {
    let request = RequestEnvelope::new(opaque(), opaque(), client, body);
    send_request(&request, 3000)
        .map_err(|_| ())?
        .result
        .map_err(|_| ())
}

#[cfg(windows)]
fn opaque() -> String {
    let mut bytes = [0_u8; 24];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("KarmaControl can only run on Windows");
}
