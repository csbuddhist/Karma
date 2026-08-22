use karma_ipc::{MAX_FRAME_BYTES, RequestEnvelope, ResponseEnvelope, decode_frame, encode_frame};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_PIPE_CONNECTED, ERROR_SEM_TIMEOUT, GENERIC_READ, GENERIC_WRITE,
            HANDLE, HLOCAL, INVALID_HANDLE_VALUE, LocalFree,
        },
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
        Storage::FileSystem::{
            CreateFileW, FILE_SHARE_MODE, FlushFileBuffers, OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
        },
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE,
            PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
            WaitNamedPipeW,
        },
    },
    core::{PCWSTR, w},
};

use crate::{SERVICE_PIPE_NAME, TransportError};

const PIPE_BUFFER_BYTES: u32 = 64 * 1024;
const LOCAL_PIPE_SDDL: PCWSTR = w!("D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)");

struct OwnedHandle(HANDLE);

// SAFETY: Windows kernel handles are valid across threads. OwnedHandle has unique ownership,
// exposes no cloning, and is moved to exactly one connection worker before any I/O occurs.
unsafe impl Send for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

struct OwnedSecurityDescriptor(PSECURITY_DESCRIPTOR);

impl Drop for OwnedSecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0.0)));
            }
        }
    }
}

pub struct PipeServer {
    handle: OwnedHandle,
}

impl PipeServer {
    pub fn create() -> Result<Self, TransportError> {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                LOCAL_PIPE_SDDL,
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(|_| TransportError::OperationFailed)?;
        let _owned_descriptor = OwnedSecurityDescriptor(descriptor);
        let mut security = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let name = wide_string(SERVICE_PIPE_NAME);
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                PIPE_BUFFER_BYTES,
                PIPE_BUFFER_BYTES,
                0,
                Some(&mut security),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(TransportError::OperationFailed);
        }
        Ok(Self {
            handle: OwnedHandle(handle),
        })
    }

    pub fn accept(&self) -> Result<(), TransportError> {
        match unsafe { ConnectNamedPipe(self.handle.0, None) } {
            Ok(()) => {}
            Err(error) if error.code().0 as u32 == ERROR_PIPE_CONNECTED.0 => {}
            Err(_) => return Err(TransportError::OperationFailed),
        }
        Ok(())
    }

    pub fn read_request(&self) -> Result<RequestEnvelope, TransportError> {
        read_message(self.handle.0)
    }

    pub fn send_response(&self, response: &ResponseEnvelope) -> Result<(), TransportError> {
        let result = write_message(self.handle.0, response).and_then(|_| {
            unsafe { FlushFileBuffers(self.handle.0) }.map_err(|_| TransportError::OperationFailed)
        });
        unsafe {
            let _ = DisconnectNamedPipe(self.handle.0);
        }
        result
    }
}

pub fn send_request(
    request: &RequestEnvelope,
    timeout_ms: u32,
) -> Result<ResponseEnvelope, TransportError> {
    let name = wide_string(SERVICE_PIPE_NAME);
    unsafe { WaitNamedPipeW(PCWSTR(name.as_ptr()), timeout_ms) }
        .ok()
        .map_err(|error| {
            if error.code().0 as u32 == ERROR_SEM_TIMEOUT.0 {
                TransportError::TimedOut
            } else {
                TransportError::ServiceUnavailable
            }
        })?;
    let handle = unsafe {
        CreateFileW(
            PCWSTR(name.as_ptr()),
            (GENERIC_READ | GENERIC_WRITE).0,
            FILE_SHARE_MODE(0),
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )
    }
    .map_err(|_| TransportError::ServiceUnavailable)?;
    let handle = OwnedHandle(handle);
    write_message(handle.0, request)?;
    read_message(handle.0)
}

fn write_message<T: serde::Serialize>(handle: HANDLE, message: &T) -> Result<(), TransportError> {
    let bytes = encode_frame(message).map_err(|_| TransportError::InvalidFrame)?;
    write_all(handle, &bytes)
}

fn read_message<T: serde::de::DeserializeOwned>(handle: HANDLE) -> Result<T, TransportError> {
    let mut prefix = [0_u8; 4];
    read_exact(handle, &mut prefix)?;
    let body_len = u32::from_le_bytes(prefix) as usize;
    if body_len > MAX_FRAME_BYTES {
        return Err(TransportError::InvalidFrame);
    }
    let mut framed = Vec::with_capacity(body_len + 4);
    framed.extend_from_slice(&prefix);
    framed.resize(body_len + 4, 0);
    read_exact(handle, &mut framed[4..])?;
    decode_frame(&framed).map_err(|_| TransportError::InvalidFrame)
}

fn write_all(handle: HANDLE, mut bytes: &[u8]) -> Result<(), TransportError> {
    while !bytes.is_empty() {
        let mut written = 0_u32;
        unsafe {
            windows::Win32::Storage::FileSystem::WriteFile(
                handle,
                Some(bytes),
                Some(&mut written),
                None,
            )
        }
        .map_err(|_| TransportError::OperationFailed)?;
        if written == 0 {
            return Err(TransportError::OperationFailed);
        }
        bytes = &bytes[written as usize..];
    }
    Ok(())
}

fn read_exact(handle: HANDLE, mut bytes: &mut [u8]) -> Result<(), TransportError> {
    while !bytes.is_empty() {
        let mut read = 0_u32;
        unsafe {
            windows::Win32::Storage::FileSystem::ReadFile(
                handle,
                Some(bytes),
                Some(&mut read),
                None,
            )
        }
        .map_err(|_| TransportError::OperationFailed)?;
        if read == 0 {
            return Err(TransportError::InvalidFrame);
        }
        let (_, remaining) = bytes.split_at_mut(read as usize);
        bytes = remaining;
    }
    Ok(())
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
