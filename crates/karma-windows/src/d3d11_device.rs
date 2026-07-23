use windows::{
    Graphics::DirectX::Direct3D11::IDirect3DDevice,
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::{D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP},
            Direct3D11::{
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION, D3D11CreateDevice,
                ID3D11Device, ID3D11DeviceContext,
            },
            Dxgi::{IDXGIAdapter, IDXGIDevice},
        },
        System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice,
    },
    core::{self, Interface},
};

use crate::WindowsAdapterError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureDriver {
    Hardware,
    Warp,
}

pub struct D3d11CaptureDevice {
    driver: CaptureDriver,
    native: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt: IDirect3DDevice,
}

fn missing_output() -> WindowsAdapterError {
    WindowsAdapterError::api("D3D11CreateDevice output", core::Error::empty())
}

fn create_for_driver(
    driver_type: D3D_DRIVER_TYPE,
) -> Result<(ID3D11Device, ID3D11DeviceContext, IDirect3DDevice), WindowsAdapterError> {
    let mut native = None;
    let mut context = None;
    // SAFETY: output pointers refer to live Option storage, the SDK version and
    // flags are documented D3D11 constants, and no explicit adapter is supplied.
    unsafe {
        D3D11CreateDevice(
            None::<&IDXGIAdapter>,
            driver_type,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut native),
            None,
            Some(&mut context),
        )
    }
    .map_err(|source| WindowsAdapterError::api("D3D11CreateDevice", source))?;

    let native = native.ok_or_else(missing_output)?;
    let context = context.ok_or_else(missing_output)?;
    let dxgi: IDXGIDevice = native
        .cast()
        .map_err(|source| WindowsAdapterError::api("ID3D11Device to IDXGIDevice", source))?;
    // SAFETY: `dxgi` is obtained by querying the successfully created D3D11
    // device for IDXGIDevice, which is the required interop input.
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }.map_err(|source| {
        WindowsAdapterError::api("CreateDirect3D11DeviceFromDXGIDevice", source)
    })?;
    let winrt = inspectable
        .cast::<IDirect3DDevice>()
        .map_err(|source| WindowsAdapterError::api("IInspectable to IDirect3DDevice", source))?;
    Ok((native, context, winrt))
}

impl D3d11CaptureDevice {
    pub fn new() -> Result<Self, WindowsAdapterError> {
        match create_for_driver(D3D_DRIVER_TYPE_HARDWARE) {
            Ok((native, context, winrt)) => Ok(Self {
                driver: CaptureDriver::Hardware,
                native,
                context,
                winrt,
            }),
            Err(_) => {
                let (native, context, winrt) = create_for_driver(D3D_DRIVER_TYPE_WARP)?;
                Ok(Self {
                    driver: CaptureDriver::Warp,
                    native,
                    context,
                    winrt,
                })
            }
        }
    }

    pub fn driver(&self) -> CaptureDriver {
        self.driver
    }

    pub fn winrt_device(&self) -> &IDirect3DDevice {
        &self.winrt
    }

    pub fn native_device(&self) -> &ID3D11Device {
        &self.native
    }

    pub fn immediate_context(&self) -> &ID3D11DeviceContext {
        &self.context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WindowsAdapterError;

    #[test]
    fn public_device_constructor_has_stable_signature() {
        let constructor: fn() -> Result<D3d11CaptureDevice, WindowsAdapterError> =
            D3d11CaptureDevice::new;
        let _ = constructor;
    }
}
