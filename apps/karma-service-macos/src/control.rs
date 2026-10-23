//! KarmaControl for macOS: asks KarmaService to authorize an uninstall or to
//! apply a staged update. Run it as root; the password is read from standard
//! input.

#[cfg(target_os = "macos")]
use std::io::Read;

#[cfg(target_os = "macos")]
use karma_ipc::{ClientKind, RequestEnvelope, ServiceRequest, ServiceResult};
#[cfg(target_os = "macos")]
use rand::{RngCore, rngs::OsRng};
#[cfg(target_os = "macos")]
use zeroize::Zeroizing;

#[cfg(target_os = "macos")]
fn main() {
    let command = std::env::args_os().nth(1);
    if command.as_deref() == Some(std::ffi::OsStr::new("version")) {
        println!("{}", karma_ipc::PRODUCT_VERSION);
        return;
    }
    let update = command.as_deref() == Some(std::ffi::OsStr::new("update"));
    if !update && command.as_deref() != Some(std::ffi::OsStr::new("shutdown")) {
        eprintln!(
            "usage: sudo KarmaControl version | shutdown | update (password is read from standard input)"
        );
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
        if update {
            ServiceRequest::ApplyStagedUpdate {
                password: password.to_string(),
            }
        } else {
            ServiceRequest::RequestShutdown {
                password: password.to_string(),
            }
        },
    );
    let response = match karma_macos_ipc::send_request(&request, 3000) {
        Ok(response) => response,
        Err(_) => {
            eprintln!("KarmaService is not reachable; operation was not requested");
            std::process::exit(4);
        }
    };
    match response.result {
        Ok(ServiceResult::Acknowledged) => println!(
            "KarmaService accepted the {} request",
            if update { "update" } else { "shutdown" }
        ),
        Ok(_) => {
            eprintln!("unexpected service response; operation was not requested");
            std::process::exit(3);
        }
        Err(_) => {
            eprintln!(
                "authentication failed or KarmaControl is not running as root; operation was not requested"
            );
            std::process::exit(3);
        }
    }
}

#[cfg(target_os = "macos")]
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

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("this KarmaControl can only run on macOS");
}
