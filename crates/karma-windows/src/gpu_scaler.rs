use std::mem::ManuallyDrop;

use karma_ai::FrameDimensions;
use thiserror::Error;
use windows::{
    Win32::{
        Foundation::RECT,
        Graphics::{
            Direct3D11::{
                D3D11_BIND_RENDER_TARGET, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_DEFAULT, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
                D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT,
                D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
                D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
                D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM,
                D3D11_VIDEO_USAGE_OPTIMAL_SPEED, D3D11_VPIV_DIMENSION_TEXTURE2D,
                D3D11_VPOV_DIMENSION_TEXTURE2D, ID3D11Device, ID3D11Texture2D, ID3D11VideoContext,
                ID3D11VideoDevice, ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator,
                ID3D11VideoProcessorOutputView,
            },
            Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_RATIONAL, DXGI_SAMPLE_DESC},
        },
    },
    core::{self, Interface},
};

use crate::{D3d11CaptureDevice, NativeCaptureTexture};

const MAXIMUM_GPU_OUTPUT_EDGE: u32 = 640;

#[derive(Debug, Error)]
pub enum GpuScalerError {
    #[error("D3D11 video processing is unsupported during {operation}")]
    Unsupported {
        operation: &'static str,
        #[source]
        source: core::Error,
    },
    #[error("D3D11 video processor does not support BGRA8 input and output")]
    UnsupportedBgra,
    #[error("D3D11 API failed during {operation}")]
    Api {
        operation: &'static str,
        #[source]
        source: core::Error,
    },
    #[error("D3D11 API returned no object during {operation}")]
    MissingOutput { operation: &'static str },
    #[error("GPU scale dimensions are invalid or exceed the bounded output")]
    InvalidDimensions,
}

impl GpuScalerError {
    fn api(operation: &'static str, source: core::Error) -> Self {
        Self::Api { operation, source }
    }
}

struct ScaleResources {
    source: FrameDimensions,
    target: FrameDimensions,
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    output_texture: ID3D11Texture2D,
    output_view: ID3D11VideoProcessorOutputView,
}

struct VideoProcessorStream {
    inner: D3D11_VIDEO_PROCESSOR_STREAM,
}

impl VideoProcessorStream {
    fn new(
        input_view: windows::Win32::Graphics::Direct3D11::ID3D11VideoProcessorInputView,
    ) -> Self {
        let mut inner = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: true.into(),
            ..Default::default()
        };
        inner.pInputSurface = ManuallyDrop::new(Some(input_view));
        Self { inner }
    }
}

impl Drop for VideoProcessorStream {
    fn drop(&mut self) {
        // SAFETY: these ManuallyDrop fields were initialized as valid Options;
        // this wrapper owns them and drops each exactly once.
        unsafe {
            ManuallyDrop::drop(&mut self.inner.pInputSurface);
            ManuallyDrop::drop(&mut self.inner.pInputSurfaceRight);
        }
    }
}

pub struct GpuFrameScaler {
    device: ID3D11Device,
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    resources: Option<ScaleResources>,
}

impl GpuFrameScaler {
    pub fn new(device: &D3d11CaptureDevice) -> Result<Self, GpuScalerError> {
        let video_device =
            device
                .native_device()
                .cast()
                .map_err(|source| GpuScalerError::Unsupported {
                    operation: "ID3D11Device to ID3D11VideoDevice",
                    source,
                })?;
        let video_context =
            device
                .immediate_context()
                .cast()
                .map_err(|source| GpuScalerError::Unsupported {
                    operation: "ID3D11DeviceContext to ID3D11VideoContext",
                    source,
                })?;
        Ok(Self {
            device: device.native_device().clone(),
            video_device,
            video_context,
            resources: None,
        })
    }

    fn create_resources(
        &self,
        source: FrameDimensions,
        target: FrameDimensions,
    ) -> Result<ScaleResources, GpuScalerError> {
        let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {
                Numerator: 1,
                Denominator: 1,
            },
            InputWidth: source.width(),
            InputHeight: source.height(),
            OutputFrameRate: DXGI_RATIONAL {
                Numerator: 1,
                Denominator: 1,
            },
            OutputWidth: target.width(),
            OutputHeight: target.height(),
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_SPEED,
        };
        // SAFETY: content is fully initialized and contains validated non-zero
        // dimensions. The video device does not retain the pointer.
        let enumerator = unsafe { self.video_device.CreateVideoProcessorEnumerator(&content) }
            .map_err(|source| GpuScalerError::Unsupported {
                operation: "ID3D11VideoDevice.CreateVideoProcessorEnumerator",
                source,
            })?;
        // SAFETY: the enumerator is live and BGRA8 is a concrete DXGI format.
        let format_support =
            unsafe { enumerator.CheckVideoProcessorFormat(DXGI_FORMAT_B8G8R8A8_UNORM) }.map_err(
                |source| {
                    GpuScalerError::api(
                        "ID3D11VideoProcessorEnumerator.CheckVideoProcessorFormat",
                        source,
                    )
                },
            )?;
        let required = (D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0
            | D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0) as u32;
        if format_support & required != required {
            return Err(GpuScalerError::UnsupportedBgra);
        }
        // SAFETY: rate conversion index zero is the baseline capability exposed
        // by a successfully created enumerator.
        let processor = unsafe { self.video_device.CreateVideoProcessor(&enumerator, 0) }.map_err(
            |source| GpuScalerError::Unsupported {
                operation: "ID3D11VideoDevice.CreateVideoProcessor",
                source,
            },
        )?;

        let output_description = D3D11_TEXTURE2D_DESC {
            Width: target.width(),
            Height: target.height(),
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut output_texture = None;
        // SAFETY: the description is fully initialized and output storage is
        // valid. No initial CPU data is supplied.
        unsafe {
            self.device
                .CreateTexture2D(&output_description, None, Some(&mut output_texture))
        }
        .map_err(|source| GpuScalerError::api("ID3D11Device.CreateTexture2D output", source))?;
        let output_texture = output_texture.ok_or(GpuScalerError::MissingOutput {
            operation: "ID3D11Device.CreateTexture2D output",
        })?;
        let output_view_description = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
            },
        };
        let mut output_view = None;
        // SAFETY: texture and enumerator are compatible with the content
        // description and output storage is valid.
        unsafe {
            self.video_device.CreateVideoProcessorOutputView(
                &output_texture,
                &enumerator,
                &output_view_description,
                Some(&mut output_view),
            )
        }
        .map_err(|source| {
            GpuScalerError::api("ID3D11VideoDevice.CreateVideoProcessorOutputView", source)
        })?;
        let output_view = output_view.ok_or(GpuScalerError::MissingOutput {
            operation: "ID3D11VideoDevice.CreateVideoProcessorOutputView",
        })?;
        Ok(ScaleResources {
            source,
            target,
            enumerator,
            processor,
            output_texture,
            output_view,
        })
    }

    fn ensure_resources(
        &mut self,
        source: FrameDimensions,
        target: FrameDimensions,
    ) -> Result<(), GpuScalerError> {
        if target.width().max(target.height()) > MAXIMUM_GPU_OUTPUT_EDGE {
            return Err(GpuScalerError::InvalidDimensions);
        }
        let recreate = self
            .resources
            .as_ref()
            .is_none_or(|resources| resources.source != source || resources.target != target);
        if recreate {
            self.resources = Some(self.create_resources(source, target)?);
        }
        Ok(())
    }

    pub fn scale(
        &mut self,
        source: &NativeCaptureTexture<'_>,
        target: FrameDimensions,
    ) -> Result<&ID3D11Texture2D, GpuScalerError> {
        let source_dimensions = source.dimensions();
        let source_right = i32::try_from(source_dimensions.width())
            .map_err(|_| GpuScalerError::InvalidDimensions)?;
        let source_bottom = i32::try_from(source_dimensions.height())
            .map_err(|_| GpuScalerError::InvalidDimensions)?;
        let target_right =
            i32::try_from(target.width()).map_err(|_| GpuScalerError::InvalidDimensions)?;
        let target_bottom =
            i32::try_from(target.height()).map_err(|_| GpuScalerError::InvalidDimensions)?;
        self.ensure_resources(source_dimensions, target)?;
        let resources = self.resources.as_ref().expect("GPU resources exist");

        let input_view_description = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };
        let mut input_view = None;
        // SAFETY: the live WGC texture remains tied to `source`, the enumerator
        // matches its content dimensions, and output storage is valid.
        unsafe {
            self.video_device.CreateVideoProcessorInputView(
                source.texture(),
                &resources.enumerator,
                &input_view_description,
                Some(&mut input_view),
            )
        }
        .map_err(|source| {
            GpuScalerError::api("ID3D11VideoDevice.CreateVideoProcessorInputView", source)
        })?;
        let input_view = input_view.ok_or(GpuScalerError::MissingOutput {
            operation: "ID3D11VideoDevice.CreateVideoProcessorInputView",
        })?;

        let source_rect = RECT {
            left: 0,
            top: 0,
            right: source_right,
            bottom: source_bottom,
        };
        let target_rect = RECT {
            left: 0,
            top: 0,
            right: target_right,
            bottom: target_bottom,
        };
        // SAFETY: processor, context and views originate from the same D3D11
        // device; rectangles are positive and bounded by their textures.
        unsafe {
            self.video_context.VideoProcessorSetOutputTargetRect(
                &resources.processor,
                true,
                Some(&target_rect),
            );
            self.video_context.VideoProcessorSetStreamFrameFormat(
                &resources.processor,
                0,
                D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            );
            self.video_context.VideoProcessorSetStreamSourceRect(
                &resources.processor,
                0,
                true,
                Some(&source_rect),
            );
            self.video_context.VideoProcessorSetStreamDestRect(
                &resources.processor,
                0,
                true,
                Some(&target_rect),
            );
        }
        let stream = VideoProcessorStream::new(input_view);
        // SAFETY: the stream owns a live input view for this call and all GPU
        // resources come from the same device and matching enumerator.
        unsafe {
            self.video_context.VideoProcessorBlt(
                &resources.processor,
                &resources.output_view,
                0,
                std::slice::from_ref(&stream.inner),
            )
        }
        .map_err(|source| GpuScalerError::api("ID3D11VideoContext.VideoProcessorBlt", source))?;
        Ok(&resources.output_texture)
    }
}

#[cfg(test)]
mod tests {
    use karma_ai::FrameDimensions;
    use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

    use super::{GpuFrameScaler, GpuScalerError};
    use crate::{D3d11CaptureDevice, NativeCaptureTexture};

    fn create_scaler(device: &D3d11CaptureDevice) -> Result<GpuFrameScaler, GpuScalerError> {
        GpuFrameScaler::new(device)
    }

    fn scale<'scaler>(
        scaler: &'scaler mut GpuFrameScaler,
        source: &NativeCaptureTexture<'_>,
        target: FrameDimensions,
    ) -> Result<&'scaler ID3D11Texture2D, GpuScalerError> {
        scaler.scale(source, target)
    }

    #[test]
    fn scaler_constructor_keeps_device_lifetime() {
        let _create: fn(&D3d11CaptureDevice) -> Result<GpuFrameScaler, GpuScalerError> =
            create_scaler;
    }

    #[test]
    fn scaler_can_be_owned_by_a_processing_worker() {
        fn require_send<T: Send>() {}
        require_send::<GpuFrameScaler>();
    }

    #[test]
    fn scaled_texture_is_borrowed_from_the_scaler() {
        let _scale: for<'scaler, 'frame> fn(
            &'scaler mut GpuFrameScaler,
            &NativeCaptureTexture<'frame>,
            FrameDimensions,
        )
            -> Result<&'scaler ID3D11Texture2D, GpuScalerError> = scale;
    }
}
