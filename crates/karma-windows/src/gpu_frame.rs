use std::marker::PhantomData;

use karma_ai::FrameDimensions;
use windows::{
    Win32::{
        Graphics::{
            Direct3D11::{D3D11_TEXTURE2D_DESC, ID3D11Texture2D},
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
        },
        System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess,
    },
    core::Interface,
};

use crate::{CapturedGpuFrame, WindowsAdapterError};

pub struct NativeCaptureTexture<'frame> {
    texture: ID3D11Texture2D,
    dimensions: FrameDimensions,
    _frame: PhantomData<&'frame CapturedGpuFrame>,
}

impl<'frame> NativeCaptureTexture<'frame> {
    pub fn from_frame(frame: &'frame CapturedGpuFrame) -> Result<Self, WindowsAdapterError> {
        let (width, height) = frame.content_size()?;
        let dimensions = FrameDimensions::new(width, height)
            .map_err(|_| WindowsAdapterError::InvalidCaptureSize)?;
        let surface = frame.surface()?;
        let access: IDirect3DDxgiInterfaceAccess = surface.cast().map_err(|source| {
            WindowsAdapterError::api("IDirect3DSurface to IDirect3DDxgiInterfaceAccess", source)
        })?;
        // SAFETY: `access` is queried from the live WGC surface and the requested
        // interface is the documented D3D11 texture backing that surface.
        let texture: ID3D11Texture2D = unsafe { access.GetInterface() }.map_err(|source| {
            WindowsAdapterError::api("IDirect3DDxgiInterfaceAccess.GetInterface", source)
        })?;
        let mut description = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: `description` is valid writable storage and `texture` remains
        // alive for the call. GetDesc does not retain the pointer.
        unsafe { texture.GetDesc(&mut description) };
        if description.Width < width || description.Height < height {
            return Err(WindowsAdapterError::CaptureTextureTooSmall);
        }
        if description.Format != DXGI_FORMAT_B8G8R8A8_UNORM {
            return Err(WindowsAdapterError::UnsupportedCaptureFormat {
                actual: description.Format.0,
            });
        }
        Ok(Self {
            texture,
            dimensions,
            _frame: PhantomData,
        })
    }

    /// The returned interface must not be retained after this wrapper is dropped.
    pub fn texture(&self) -> &ID3D11Texture2D {
        &self.texture
    }

    pub fn dimensions(&self) -> FrameDimensions {
        self.dimensions
    }
}

#[cfg(test)]
mod tests {
    use super::NativeCaptureTexture;
    use crate::{CapturedGpuFrame, WindowsAdapterError};

    fn convert(frame: &CapturedGpuFrame) -> Result<NativeCaptureTexture<'_>, WindowsAdapterError> {
        NativeCaptureTexture::from_frame(frame)
    }

    #[test]
    fn native_texture_conversion_has_a_stable_signature() {
        let _convert: for<'frame> fn(
            &'frame CapturedGpuFrame,
        )
            -> Result<NativeCaptureTexture<'frame>, WindowsAdapterError> = convert;
    }

    #[test]
    fn native_texture_can_cross_into_the_processing_worker() {
        fn require_send<T: Send>() {}
        require_send::<NativeCaptureTexture<'static>>();
    }
}
