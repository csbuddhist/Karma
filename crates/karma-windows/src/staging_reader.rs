use karma_ai::FrameDimensions;
use thiserror::Error;

const MAXIMUM_MAPPED_EDGE: u32 = 640;
const MAXIMUM_MAPPED_BYTES: usize = 640 * 640 * 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum MappedFrameError {
    #[error("mapped frame edge {actual} exceeds maximum {maximum}")]
    EdgeExceeded { maximum: u32, actual: u32 },
    #[error("mapped row pitch {actual} is smaller than {minimum}")]
    RowPitchTooSmall { minimum: usize, actual: usize },
    #[error("mapped source length {actual} is smaller than {minimum}")]
    SourceTooShort { minimum: usize, actual: usize },
    #[error("mapped frame arithmetic overflow")]
    ArithmeticOverflow,
    #[error("mapped tight frame length {actual} exceeds maximum {maximum}")]
    ByteLimitExceeded { maximum: usize, actual: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappedBgraLayout {
    dimensions: FrameDimensions,
    row_pitch: usize,
    tight_stride: usize,
    mapped_len: usize,
}

impl MappedBgraLayout {
    pub fn new(dimensions: FrameDimensions, row_pitch: usize) -> Result<Self, MappedFrameError> {
        Self::new_with_limits(
            dimensions,
            row_pitch,
            MAXIMUM_MAPPED_EDGE,
            MAXIMUM_MAPPED_BYTES,
        )
    }

    fn new_with_limits(
        dimensions: FrameDimensions,
        row_pitch: usize,
        maximum_edge: u32,
        maximum_bytes: usize,
    ) -> Result<Self, MappedFrameError> {
        let actual_edge = dimensions.width().max(dimensions.height());
        if actual_edge > maximum_edge {
            return Err(MappedFrameError::EdgeExceeded {
                maximum: maximum_edge,
                actual: actual_edge,
            });
        }
        let tight_stride = usize::try_from(dimensions.width())
            .map_err(|_| MappedFrameError::ArithmeticOverflow)?
            .checked_mul(4)
            .ok_or(MappedFrameError::ArithmeticOverflow)?;
        if row_pitch < tight_stride {
            return Err(MappedFrameError::RowPitchTooSmall {
                minimum: tight_stride,
                actual: row_pitch,
            });
        }
        let height = usize::try_from(dimensions.height())
            .map_err(|_| MappedFrameError::ArithmeticOverflow)?;
        let tight_len = tight_stride
            .checked_mul(height)
            .ok_or(MappedFrameError::ArithmeticOverflow)?;
        if tight_len > maximum_bytes {
            return Err(MappedFrameError::ByteLimitExceeded {
                maximum: maximum_bytes,
                actual: tight_len,
            });
        }
        let mapped_len = row_pitch
            .checked_mul(height)
            .ok_or(MappedFrameError::ArithmeticOverflow)?;
        Ok(Self {
            dimensions,
            row_pitch,
            tight_stride,
            mapped_len,
        })
    }

    pub fn mapped_len(self) -> usize {
        self.mapped_len
    }

    pub fn tight_stride(self) -> usize {
        self.tight_stride
    }

    pub fn copy_tight(self, source: &[u8]) -> Result<Vec<u8>, MappedFrameError> {
        if source.len() < self.mapped_len {
            return Err(MappedFrameError::SourceTooShort {
                minimum: self.mapped_len,
                actual: source.len(),
            });
        }
        let height = usize::try_from(self.dimensions.height())
            .map_err(|_| MappedFrameError::ArithmeticOverflow)?;
        let output_len = self
            .tight_stride
            .checked_mul(height)
            .ok_or(MappedFrameError::ArithmeticOverflow)?;
        let mut output = vec![0; output_len];
        for row in 0..height {
            let source_start = row
                .checked_mul(self.row_pitch)
                .ok_or(MappedFrameError::ArithmeticOverflow)?;
            let source_end = source_start
                .checked_add(self.tight_stride)
                .ok_or(MappedFrameError::ArithmeticOverflow)?;
            let destination_start = row
                .checked_mul(self.tight_stride)
                .ok_or(MappedFrameError::ArithmeticOverflow)?;
            let destination_end = destination_start
                .checked_add(self.tight_stride)
                .ok_or(MappedFrameError::ArithmeticOverflow)?;
            output[destination_start..destination_end]
                .copy_from_slice(&source[source_start..source_end]);
        }
        Ok(output)
    }
}

#[cfg(windows)]
mod native {
    use std::slice;

    use karma_ai::{BgraFrame, FrameDimensions};
    use karma_domain::MonitorId;
    use windows::Win32::Graphics::{
        Direct3D11::{
            D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC,
            D3D11_USAGE_STAGING, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
        },
        Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC},
    };

    use super::{MappedBgraLayout, MappedFrameError};
    use crate::{D3d11CaptureDevice, WindowsAdapterError};

    const MAXIMUM_FALLBACK_EDGE: u32 = 16_384;
    const MAXIMUM_FALLBACK_BYTES: usize = 256 * 1024 * 1024;

    struct MapGuard<'a> {
        context: &'a ID3D11DeviceContext,
        texture: &'a ID3D11Texture2D,
    }

    impl Drop for MapGuard<'_> {
        fn drop(&mut self) {
            // SAFETY: this guard is created only after a successful Map of
            // subresource zero and is dropped exactly once.
            unsafe { self.context.Unmap(self.texture, 0) };
        }
    }

    pub struct StagingTextureReader {
        device: ID3D11Device,
        context: ID3D11DeviceContext,
        staging: Option<(FrameDimensions, ID3D11Texture2D)>,
    }

    impl StagingTextureReader {
        pub fn new(device: &D3d11CaptureDevice) -> Self {
            Self {
                device: device.native_device().clone(),
                context: device.immediate_context().clone(),
                staging: None,
            }
        }

        fn ensure_staging(
            &mut self,
            dimensions: FrameDimensions,
        ) -> Result<&ID3D11Texture2D, WindowsAdapterError> {
            let recreate = self
                .staging
                .as_ref()
                .is_none_or(|(current, _)| *current != dimensions);
            if recreate {
                let description = D3D11_TEXTURE2D_DESC {
                    Width: dimensions.width(),
                    Height: dimensions.height(),
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    SampleDesc: DXGI_SAMPLE_DESC {
                        Count: 1,
                        Quality: 0,
                    },
                    Usage: D3D11_USAGE_STAGING,
                    BindFlags: 0,
                    CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                    MiscFlags: 0,
                };
                let mut texture = None;
                // SAFETY: the description is fully initialized, no initial data
                // is supplied, and the output points to live Option storage.
                unsafe {
                    self.device
                        .CreateTexture2D(&description, None, Some(&mut texture))
                }
                .map_err(|source| {
                    WindowsAdapterError::api("ID3D11Device.CreateTexture2D staging", source)
                })?;
                let texture = texture.ok_or_else(|| {
                    WindowsAdapterError::api(
                        "ID3D11Device.CreateTexture2D staging output",
                        windows::core::Error::empty(),
                    )
                })?;
                self.staging = Some((dimensions, texture));
            }
            Ok(&self.staging.as_ref().expect("staging texture exists").1)
        }

        fn read_with_limits(
            &mut self,
            monitor_id: MonitorId,
            captured_at_ms: i64,
            source: &ID3D11Texture2D,
            dimensions: FrameDimensions,
            maximum_edge: u32,
            maximum_bytes: usize,
        ) -> Result<BgraFrame, WindowsAdapterError> {
            let mut source_description = D3D11_TEXTURE2D_DESC::default();
            // SAFETY: the description is writable and the source is live.
            unsafe { source.GetDesc(&mut source_description) };
            if source_description.Width != dimensions.width()
                || source_description.Height != dimensions.height()
                || source_description.Format != DXGI_FORMAT_B8G8R8A8_UNORM
            {
                return Err(WindowsAdapterError::StagingSourceMismatch);
            }

            let context = self.context.clone();
            let staging = self.ensure_staging(dimensions)?;
            // SAFETY: source and staging have matching dimensions and BGRA8
            // format; both resources remain live through Map and row copy.
            unsafe { context.CopyResource(staging, source) };
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            // SAFETY: staging was created with CPU read access and the output
            // mapping structure is valid for the duration of the call.
            unsafe { context.Map(staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }
                .map_err(|source| WindowsAdapterError::api("ID3D11DeviceContext.Map", source))?;
            let _guard = MapGuard {
                context: &context,
                texture: staging,
            };
            if mapped.pData.is_null() {
                return Err(WindowsAdapterError::api(
                    "ID3D11DeviceContext.Map output",
                    windows::core::Error::empty(),
                ));
            }
            let layout = MappedBgraLayout::new_with_limits(
                dimensions,
                mapped.RowPitch as usize,
                maximum_edge,
                maximum_bytes,
            )
            .map_err(WindowsAdapterError::MappedFrame)?;
            // SAFETY: a successful Map exposes at least RowPitch bytes for each
            // texture row; checked layout arithmetic determines the exact span.
            let source_pixels =
                unsafe { slice::from_raw_parts(mapped.pData.cast::<u8>(), layout.mapped_len()) };
            let pixels = layout
                .copy_tight(source_pixels)
                .map_err(WindowsAdapterError::MappedFrame)?;
            BgraFrame::new(
                monitor_id,
                captured_at_ms,
                dimensions,
                layout.tight_stride(),
                pixels,
            )
            .map_err(WindowsAdapterError::FrameData)
        }

        pub fn read(
            &mut self,
            monitor_id: MonitorId,
            captured_at_ms: i64,
            source: &ID3D11Texture2D,
            dimensions: FrameDimensions,
        ) -> Result<BgraFrame, WindowsAdapterError> {
            self.read_with_limits(
                monitor_id,
                captured_at_ms,
                source,
                dimensions,
                super::MAXIMUM_MAPPED_EDGE,
                super::MAXIMUM_MAPPED_BYTES,
            )
        }

        pub(crate) fn read_source(
            &mut self,
            monitor_id: MonitorId,
            captured_at_ms: i64,
            source: &ID3D11Texture2D,
            dimensions: FrameDimensions,
        ) -> Result<BgraFrame, WindowsAdapterError> {
            self.read_with_limits(
                monitor_id,
                captured_at_ms,
                source,
                dimensions,
                MAXIMUM_FALLBACK_EDGE,
                MAXIMUM_FALLBACK_BYTES,
            )
        }
    }

    impl From<MappedFrameError> for WindowsAdapterError {
        fn from(value: MappedFrameError) -> Self {
            Self::MappedFrame(value)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn create_reader(device: &D3d11CaptureDevice) -> StagingTextureReader {
            StagingTextureReader::new(device)
        }

        #[test]
        fn staging_reader_constructor_keeps_device_lifetime() {
            let _create: fn(&D3d11CaptureDevice) -> StagingTextureReader = create_reader;
        }

        #[test]
        fn staging_reader_can_be_owned_by_a_processing_worker() {
            fn require_send<T: Send>() {}
            require_send::<StagingTextureReader>();
        }
    }
}

#[cfg(windows)]
pub use native::StagingTextureReader;

#[cfg(test)]
mod tests {
    use karma_ai::FrameDimensions;

    use super::{MappedBgraLayout, MappedFrameError};

    #[test]
    fn mapped_layout_accepts_padding_and_copies_only_active_pixels() {
        let dimensions = FrameDimensions::new(2, 2).unwrap();
        let layout = MappedBgraLayout::new(dimensions, 12).unwrap();
        let source = [
            1, 2, 3, 4, 5, 6, 7, 8, 99, 99, 99, 99, 9, 10, 11, 12, 13, 14, 15, 16, 88, 88, 88, 88,
        ];
        assert_eq!(
            layout.copy_tight(&source).unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }

    #[test]
    fn mapped_layout_rejects_short_pitch_and_source() {
        let dimensions = FrameDimensions::new(2, 2).unwrap();
        assert_eq!(
            MappedBgraLayout::new(dimensions, 7),
            Err(MappedFrameError::RowPitchTooSmall {
                minimum: 8,
                actual: 7,
            })
        );
        let layout = MappedBgraLayout::new(dimensions, 8).unwrap();
        assert_eq!(
            layout.copy_tight(&[0; 15]),
            Err(MappedFrameError::SourceTooShort {
                minimum: 16,
                actual: 15,
            })
        );
    }

    #[test]
    fn mapped_layout_rejects_unbounded_output() {
        let dimensions = FrameDimensions::new(641, 1).unwrap();
        assert_eq!(
            MappedBgraLayout::new(dimensions, 641 * 4),
            Err(MappedFrameError::EdgeExceeded {
                maximum: 640,
                actual: 641,
            })
        );
    }
}
