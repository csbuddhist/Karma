//! ScreenCaptureKit capture of one display.
//!
//! ScreenCaptureKit scales each display on the GPU to the frame size the
//! shared preparer uses (640 pixels on the longest edge) and delivers BGRA
//! frames at most four times a second, matching the Windows pipeline's
//! admission rate. Only complete frames are copied, into zeroizing memory;
//! the newest frame replaces any frame the agent has not taken yet.
//!
//! ScreenCaptureKit objects stay on the thread that started the stream. Frame
//! workers receive a [`FrameSource`], which shares only the mailbox and status.

use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU8, Ordering},
        mpsc::sync_channel,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use block2::RcBlock;
use dispatch2::{DispatchQueue, DispatchQueueAttr, DispatchRetained};
use karma_ai::{BgraFrame, FrameDimensions, FramePreparationConfig, LatestFrameMailbox};
use karma_domain::MonitorId;
use objc2::{
    AnyThread, DefinedClass, define_class, msg_send, rc::Retained, runtime::ProtocolObject,
};
use objc2_core_foundation::{CFDictionary, CFNumber, CFString, CFType};
use objc2_core_graphics::CGDirectDisplayID;
use objc2_core_media::{CMSampleBuffer, CMTime, CMTimeFlags};
use objc2_core_video::{
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress,
    CVPixelBufferLockFlags, CVPixelBufferUnlockBaseAddress, kCVPixelFormatType_32BGRA,
    kCVReturnSuccess,
};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol, NSString};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCFrameStatus, SCShareableContent, SCStream, SCStreamConfiguration,
    SCStreamDelegate, SCStreamErrorCode, SCStreamFrameInfoStatus, SCStreamOutput,
    SCStreamOutputType, SCWindow,
};

use crate::{CaptureError, display::DisplaySnapshot};

const FRAMES_PER_SECOND: i32 = 4;
const QUEUE_DEPTH: isize = 3;
const COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamStatus {
    Starting,
    Running,
    Stopped,
    /// Screen Recording is not allowed for this process.
    AccessDenied,
    Failed,
}

impl StreamStatus {
    fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Starting,
            1 => Self::Running,
            2 => Self::Stopped,
            3 => Self::AccessDenied,
            _ => Self::Failed,
        }
    }

    fn raw(self) -> u8 {
        match self {
            Self::Starting => 0,
            Self::Running => 1,
            Self::Stopped => 2,
            Self::AccessDenied => 3,
            Self::Failed => 4,
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, Self::Starting | Self::Running)
    }
}

/// State shared between the capture queue and the agent's frame worker.
struct StreamShared {
    monitor_id: MonitorId,
    latest: LatestFrameMailbox<BgraFrame>,
    status: AtomicU8,
    /// Set when a frame or status change arrives, cleared by the waiter.
    signal: Mutex<bool>,
    wake: Condvar,
}

impl StreamShared {
    fn notify(&self) {
        *self
            .signal
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        self.wake.notify_all();
    }

    fn publish(&self, frame: BgraFrame) {
        self.latest.push(frame);
        self.notify();
    }

    fn status(&self) -> StreamStatus {
        StreamStatus::from_raw(self.status.load(Ordering::Acquire))
    }

    /// Records `status` unless the stream already ended for a stated reason.
    fn set_status(&self, status: StreamStatus) {
        let _ = self
            .status
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                StreamStatus::from_raw(current)
                    .is_active()
                    .then_some(status.raw())
            });
        self.notify();
    }
}

/// The agent's view of one display stream; safe to move to a worker thread.
#[derive(Clone)]
pub struct FrameSource {
    shared: Arc<StreamShared>,
}

impl FrameSource {
    pub fn monitor_id(&self) -> &MonitorId {
        &self.shared.monitor_id
    }

    pub fn status(&self) -> StreamStatus {
        self.shared.status()
    }

    /// The newest frame, waiting up to `timeout` for one to arrive.
    pub fn next_frame(&self, timeout: Duration) -> Option<BgraFrame> {
        if let Some(frame) = self.shared.latest.take() {
            return Some(frame);
        }
        let signalled = self
            .shared
            .signal
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (mut signalled, _) = self
            .shared
            .wake
            .wait_timeout_while(signalled, timeout, |signalled| {
                !*signalled && self.shared.latest.is_empty()
            })
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *signalled = false;
        drop(signalled);
        self.shared.latest.take()
    }
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and StreamObserver
    // does not implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "KarmaStreamObserver"]
    #[ivars = Arc<StreamShared>]
    struct StreamObserver;

    unsafe impl NSObjectProtocol for StreamObserver {}

    unsafe impl SCStreamOutput for StreamObserver {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        fn stream_did_output_sample_buffer(
            &self,
            _stream: &SCStream,
            sample_buffer: &CMSampleBuffer,
            kind: SCStreamOutputType,
        ) {
            if kind == SCStreamOutputType::Screen {
                if let Some(frame) = copy_frame(&self.ivars().monitor_id, sample_buffer) {
                    self.ivars().publish(frame);
                }
            }
        }
    }

    unsafe impl SCStreamDelegate for StreamObserver {
        #[unsafe(method(stream:didStopWithError:))]
        fn stream_did_stop_with_error(&self, _stream: &SCStream, error: &NSError) {
            self.ivars().set_status(match error_for(error.code()) {
                CaptureError::AccessDenied => StreamStatus::AccessDenied,
                _ => StreamStatus::Failed,
            });
        }
    }
);

impl StreamObserver {
    fn new(shared: Arc<StreamShared>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(shared);
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

/// A running ScreenCaptureKit stream of one display.
pub struct DisplayStream {
    stream: Retained<SCStream>,
    _observer: Retained<StreamObserver>,
    _queue: DispatchRetained<DispatchQueue>,
    shared: Arc<StreamShared>,
    stopped: bool,
}

impl DisplayStream {
    /// Starts capturing `display`, waiting until ScreenCaptureKit confirms.
    pub fn start(display: &DisplaySnapshot) -> Result<Self, CaptureError> {
        let content_display = shareable_display(display.id)?;
        let shared = Arc::new(StreamShared {
            monitor_id: MonitorId(display.monitor_id()),
            latest: LatestFrameMailbox::new(),
            status: AtomicU8::new(StreamStatus::Starting.raw()),
            signal: Mutex::new(false),
            wake: Condvar::new(),
        });
        // SAFETY: SCDisplay and SCWindow arrays come from ScreenCaptureKit.
        let filter = unsafe {
            SCContentFilter::initWithDisplay_excludingWindows(
                SCContentFilter::alloc(),
                &content_display,
                &NSArray::<SCWindow>::new(),
            )
        };
        // SAFETY: ScreenCaptureKit reports display sizes in points.
        let (width, height) = unsafe { (content_display.width(), content_display.height()) };
        let target = output_size(width, height)?;
        let configuration = unsafe { SCStreamConfiguration::new() };
        // SAFETY: plain property setters on a configuration this thread owns.
        unsafe {
            configuration.setWidth(target.width() as usize);
            configuration.setHeight(target.height() as usize);
            configuration.setPixelFormat(kCVPixelFormatType_32BGRA);
            configuration.setMinimumFrameInterval(CMTime {
                value: 1,
                timescale: FRAMES_PER_SECOND,
                flags: CMTimeFlags::Valid,
                epoch: 0,
            });
            configuration.setQueueDepth(QUEUE_DEPTH);
            configuration.setShowsCursor(false);
        }
        let observer = StreamObserver::new(Arc::clone(&shared));
        // SAFETY: the filter, configuration, and delegate outlive the call;
        // the delegate is retained by `DisplayStream` for the stream's life.
        let stream = unsafe {
            SCStream::initWithFilter_configuration_delegate(
                SCStream::alloc(),
                &filter,
                &configuration,
                Some(ProtocolObject::from_ref(&*observer)),
            )
        };
        let queue = DispatchQueue::new(
            &format!("com.karma.capture.display-{}", display.id),
            DispatchQueueAttr::SERIAL,
        );
        // SAFETY: the serial queue and output are retained by `DisplayStream`.
        unsafe {
            stream.addStreamOutput_type_sampleHandlerQueue_error(
                ProtocolObject::from_ref(&*observer),
                SCStreamOutputType::Screen,
                Some(&queue),
            )
        }
        .map_err(|_| CaptureError::StreamFailed)?;
        let (sender, receiver) = sync_channel(1);
        let started = RcBlock::new(move |error: *mut NSError| {
            // SAFETY: ScreenCaptureKit passes null or a valid error.
            let code = unsafe { error.as_ref() }.map(|error| error.code());
            let _ = sender.try_send(code);
        });
        // SAFETY: the completion handler is a valid block.
        unsafe { stream.startCaptureWithCompletionHandler(Some(&started)) };
        match receiver.recv_timeout(COMPLETION_TIMEOUT) {
            Ok(None) => {}
            Ok(Some(code)) => return Err(error_for(code)),
            Err(_) => return Err(CaptureError::TimedOut),
        }
        shared.set_status(StreamStatus::Running);
        Ok(Self {
            stream,
            _observer: observer,
            _queue: queue,
            shared,
            stopped: false,
        })
    }

    pub fn frames(&self) -> FrameSource {
        FrameSource {
            shared: Arc::clone(&self.shared),
        }
    }

    pub fn status(&self) -> StreamStatus {
        self.shared.status()
    }

    pub fn stop(&mut self) {
        if std::mem::replace(&mut self.stopped, true) {
            return;
        }
        let (sender, receiver) = sync_channel(1);
        let stopped = RcBlock::new(move |_error: *mut NSError| {
            let _ = sender.try_send(());
        });
        // SAFETY: the completion handler is a valid block.
        unsafe { self.stream.stopCaptureWithCompletionHandler(Some(&stopped)) };
        let _ = receiver.recv_timeout(COMPLETION_TIMEOUT);
        self.shared.set_status(StreamStatus::Stopped);
    }
}

impl Drop for DisplayStream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Moves the display a completion handler found to the requesting thread.
struct SendDisplay(Retained<SCDisplay>);

// SAFETY: Objective-C reference counting is atomic, and the SCDisplay is an
// immutable description that only the receiving thread uses afterwards.
unsafe impl Send for SendDisplay {}

fn shareable_display(id: CGDirectDisplayID) -> Result<Retained<SCDisplay>, CaptureError> {
    let (sender, receiver) = sync_channel::<Result<SendDisplay, CaptureError>>(1);
    let handler = RcBlock::new(
        move |content: *mut SCShareableContent, error: *mut NSError| {
            // SAFETY: ScreenCaptureKit passes null or valid objects.
            let result = match unsafe { content.as_ref() } {
                Some(content) => unsafe { content.displays() }
                    .iter()
                    .find(|display| unsafe { display.displayID() } == id)
                    .map(SendDisplay)
                    .ok_or(CaptureError::DisplayNotFound),
                None => Err(unsafe { error.as_ref() }
                    .map_or(CaptureError::StreamFailed, |error| error_for(error.code()))),
            };
            let _ = sender.try_send(result);
        },
    );
    // SAFETY: the completion handler is a valid block.
    unsafe { SCShareableContent::getShareableContentWithCompletionHandler(&handler) };
    receiver
        .recv_timeout(COMPLETION_TIMEOUT)
        .map_err(|_| CaptureError::TimedOut)?
        .map(|display| display.0)
}

/// The stream size for a display of `width` x `height` points.
fn output_size(width: isize, height: isize) -> Result<FrameDimensions, CaptureError> {
    let width = u32::try_from(width).map_err(|_| CaptureError::DisplayNotFound)?;
    let height = u32::try_from(height).map_err(|_| CaptureError::DisplayNotFound)?;
    let source = FrameDimensions::new(width, height).map_err(|_| CaptureError::DisplayNotFound)?;
    FramePreparationConfig::default()
        .target(source)
        .map_err(|_| CaptureError::DisplayNotFound)
}

fn error_for(code: isize) -> CaptureError {
    if code == SCStreamErrorCode::UserDeclined.0 {
        CaptureError::AccessDenied
    } else {
        CaptureError::StreamFailed
    }
}

/// Copies a complete BGRA frame out of ScreenCaptureKit's pixel buffer.
fn copy_frame(monitor_id: &MonitorId, sample_buffer: &CMSampleBuffer) -> Option<BgraFrame> {
    if !frame_is_complete(sample_buffer) {
        return None;
    }
    // SAFETY: the sample buffer is valid for the duration of the callback.
    let image = unsafe { sample_buffer.image_buffer() }?;
    if CVPixelBufferGetPixelFormatType(&image) != kCVPixelFormatType_32BGRA {
        return None;
    }
    // SAFETY: the buffer is locked read-only around every access below.
    if unsafe { CVPixelBufferLockBaseAddress(&image, CVPixelBufferLockFlags::ReadOnly) }
        != kCVReturnSuccess
    {
        return None;
    }
    let width = CVPixelBufferGetWidth(&image);
    let height = CVPixelBufferGetHeight(&image);
    let stride = CVPixelBufferGetBytesPerRow(&image);
    let base = CVPixelBufferGetBaseAddress(&image);
    let pixels = stride.checked_mul(height).and_then(|length| {
        // SAFETY: a locked BGRA buffer holds `height` rows of `stride` bytes.
        (!base.is_null())
            .then(|| unsafe { std::slice::from_raw_parts(base.cast::<u8>(), length) }.to_vec())
    });
    // SAFETY: balances the successful lock above.
    unsafe { CVPixelBufferUnlockBaseAddress(&image, CVPixelBufferLockFlags::ReadOnly) };
    let dimensions =
        FrameDimensions::new(u32::try_from(width).ok()?, u32::try_from(height).ok()?).ok()?;
    BgraFrame::new(
        monitor_id.clone(),
        unix_time_ms(),
        dimensions,
        stride,
        pixels?,
    )
    .ok()
}

/// Whether ScreenCaptureKit marked the sample as a complete new frame rather
/// than an idle, blank, or suspended one.
fn frame_is_complete(sample_buffer: &CMSampleBuffer) -> bool {
    // SAFETY: the sample buffer is valid for the duration of the callback.
    let Some(attachments) = (unsafe { sample_buffer.sample_attachments_array(false) }) else {
        return false;
    };
    // SAFETY: sample attachments are dictionaries keyed by strings.
    let attachments = unsafe { attachments.cast_unchecked::<CFDictionary<CFString, CFType>>() };
    let Some(first) = attachments.get(0) else {
        return false;
    };
    // SAFETY: NSString is toll-free bridged with CFString, and the key is a
    // ScreenCaptureKit constant.
    let key = unsafe { &*(SCStreamFrameInfoStatus as *const NSString).cast::<CFString>() };
    first
        .get(key)
        .and_then(|status| status.downcast::<CFNumber>().ok())
        .and_then(|status| status.as_isize())
        == Some(SCFrameStatus::Complete.0)
}

fn unix_time_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_are_sized_like_prepared_frames() {
        assert_eq!(
            output_size(1512, 982).unwrap(),
            FrameDimensions::new(640, 416).unwrap()
        );
        assert_eq!(
            output_size(600, 400).unwrap(),
            FrameDimensions::new(600, 400).unwrap()
        );
        assert!(output_size(0, 400).is_err());
    }

    #[test]
    fn a_declined_permission_is_reported_as_access_denied() {
        assert_eq!(
            error_for(SCStreamErrorCode::UserDeclined.0),
            CaptureError::AccessDenied
        );
        assert_eq!(
            error_for(SCStreamErrorCode::UserStopped.0),
            CaptureError::StreamFailed
        );
    }

    #[test]
    fn stream_status_ends_once() {
        let shared = StreamShared {
            monitor_id: MonitorId("display-1".into()),
            latest: LatestFrameMailbox::new(),
            status: AtomicU8::new(StreamStatus::Starting.raw()),
            signal: Mutex::new(false),
            wake: Condvar::new(),
        };
        shared.set_status(StreamStatus::Running);
        shared.set_status(StreamStatus::AccessDenied);
        shared.set_status(StreamStatus::Stopped);
        assert_eq!(shared.status(), StreamStatus::AccessDenied);
    }
}
