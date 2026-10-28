//! The macOS backend: ScreenCaptureKit for the windows and their frames, the
//! workspace for which client is in front. Neither needs the Accessibility
//! permission; ScreenCaptureKit asks for Screen Recording on first use.

use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use dispatch2::{DispatchQoS, DispatchQueue, DispatchRetained, GlobalQueueIdentifier};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{define_class, msg_send, AllocAnyThread, DefinedClass};
use objc2_app_kit::NSWorkspace;
use objc2_core_foundation::CFArray;
use objc2_core_media::{CMSampleBuffer, CMTime};
use objc2_core_video::{
    kCVPixelFormatType_32BGRA, CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSNumber, NSObject, NSObjectProtocol, NSString};
use objc2_screen_capture_kit::{
    SCContentFilter, SCFrameStatus, SCShareableContent, SCStream, SCStreamConfiguration, SCStreamFrameInfoStatus,
    SCStreamOutput, SCStreamOutputType, SCWindow,
};

use crate::{CaptureError, Frame, GameWindow, Rect, Sink, DOFUS_BUNDLE_ID};

/// How long ScreenCaptureKit gets to answer a request.
const TIMEOUT: Duration = Duration::from_secs(5);
/// The widest frame read, in pixels: a 5K window costs no more than a 2.5K one.
const MAX_WIDTH: f64 = 2560.0;
/// A smaller window is a launcher, a menu or a tooltip, not the game.
const MIN_SIDE: f64 = 200.0;
/// `SCStreamErrorUserDeclined`: the Screen Recording permission is missing.
const USER_DECLINED: isize = -3801;

/// ScreenCaptureKit objects cross between its queues and ours. They are
/// thread-safe; objc2 cannot know it.
struct Sendable<T>(T);
// SAFETY: only wraps ScreenCaptureKit and dispatch objects, which Apple
// documents as usable from any queue.
unsafe impl<T> Send for Sendable<T> {}

fn error_from(e: &NSError) -> CaptureError {
    if e.code() == USER_DECLINED {
        CaptureError::Denied
    } else {
        CaptureError::Failed(e.localizedDescription().to_string())
    }
}

/// The shareable content, waited for: ScreenCaptureKit answers on its own
/// queue. Windows on other spaces are included, the full-screen game among
/// them.
fn shareable_content() -> Result<Retained<SCShareableContent>, CaptureError> {
    let (tx, rx) = mpsc::channel();
    let handler = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
        // SAFETY: ScreenCaptureKit hands over valid objects or null.
        let result = match unsafe { Retained::retain(content) } {
            Some(content) => Ok(Sendable(content)),
            None => Err(unsafe { Retained::retain(error) }
                .map_or_else(|| CaptureError::Failed("no shareable content".into()), |e| error_from(&e))),
        };
        let _ = tx.send(result);
    });
    // SAFETY: the handler lives until ScreenCaptureKit releases it.
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            true, false, &handler,
        );
    }
    match rx.recv_timeout(TIMEOUT) {
        Ok(result) => result.map(|c| c.0),
        Err(_) => Err(CaptureError::Failed("ScreenCaptureKit did not answer".into())),
    }
}

/// A game client's window, or `None` for any other window.
fn game_window(w: &SCWindow) -> Option<GameWindow> {
    // SAFETY: plain property reads on a live SCWindow.
    unsafe {
        let app = w.owningApplication()?;
        if app.bundleIdentifier().to_string() != DOFUS_BUNDLE_ID || w.windowLayer() != 0 {
            return None;
        }
        let frame = w.frame();
        if frame.size.width < MIN_SIDE || frame.size.height < MIN_SIDE {
            return None;
        }
        Some(GameWindow {
            id: w.windowID(),
            pid: app.processID(),
            title: w.title().map(|t| t.to_string()).unwrap_or_default(),
            frame: Rect { x: frame.origin.x, y: frame.origin.y, width: frame.size.width, height: frame.size.height },
        })
    }
}

pub fn game_windows() -> Result<Vec<GameWindow>, CaptureError> {
    let content = shareable_content()?;
    // SAFETY: a property read on the live content.
    let windows = unsafe { content.windows() };
    Ok(windows.iter().filter_map(|w| game_window(&w)).collect())
}

pub fn active_game_window(windows: &[GameWindow]) -> Option<u32> {
    let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    if app.bundleIdentifier()?.to_string() != DOFUS_BUNDLE_ID {
        return None;
    }
    let pid = app.processIdentifier();
    // Each client runs one character: its largest window is its game.
    windows
        .iter()
        .filter(|w| w.pid == pid)
        .max_by(|a, b| (a.frame.width * a.frame.height).total_cmp(&(b.frame.width * b.frame.height)))
        .map(|w| w.id)
}

/// Whether ScreenCaptureKit marks the sample as a complete new frame: when the
/// window does not change, it sends idle samples without new content.
fn is_complete(sample: &CMSampleBuffer) -> bool {
    // SAFETY: reading the attachments of a live sample buffer.
    let Some(attachments) = (unsafe { sample.sample_attachments_array(false) }) else {
        return false;
    };
    // SAFETY: CFArray and CFDictionary are toll-free bridged with NSArray and
    // NSDictionary, and the attachments are dictionaries keyed by strings.
    let attachments: &NSArray<NSDictionary<NSString, AnyObject>> =
        unsafe { &*(&*attachments as *const CFArray).cast() };
    let Some(first) = attachments.firstObject() else {
        return false;
    };
    // SAFETY: the key is a constant string ScreenCaptureKit exports.
    let Some(status) = first.objectForKey(unsafe { SCStreamFrameInfoStatus }) else {
        return false;
    };
    status.downcast_ref::<NSNumber>().is_some_and(|n| n.integerValue() == SCFrameStatus::Complete.0)
}

/// The pixels of a complete sample, copied out of its buffer.
fn read_frame(sample: &CMSampleBuffer, window: u32) -> Option<Frame> {
    if !is_complete(sample) {
        return None;
    }
    // SAFETY: the image buffer of a live sample buffer.
    let image = unsafe { sample.image_buffer() }?;
    let pixels: &CVPixelBuffer = &image;
    // SAFETY: the base address is read only between lock and unlock, within
    // the buffer's rows.
    unsafe {
        if CVPixelBufferLockBaseAddress(pixels, CVPixelBufferLockFlags::ReadOnly) != 0 {
            return None;
        }
        let (width, height) = (CVPixelBufferGetWidth(pixels), CVPixelBufferGetHeight(pixels));
        let stride = CVPixelBufferGetBytesPerRow(pixels);
        let base = CVPixelBufferGetBaseAddress(pixels).cast::<u8>();
        let mut bgra = Vec::with_capacity(width * height * 4);
        if !base.is_null() && stride >= width * 4 {
            for row in 0..height {
                bgra.extend_from_slice(std::slice::from_raw_parts(base.add(row * stride), width * 4));
            }
        }
        CVPixelBufferUnlockBaseAddress(pixels, CVPixelBufferLockFlags::ReadOnly);
        if bgra.is_empty() {
            return None;
        }
        Some(Frame { window, width: u32::try_from(width).ok()?, height: u32::try_from(height).ok()?, bgra })
    }
}

struct OutputIvars {
    window: u32,
    sink: Sink,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and FrameOutput does
    // not implement Drop.
    #[unsafe(super(NSObject))]
    #[name = "KroztiesFrameOutput"]
    #[ivars = OutputIvars]
    struct FrameOutput;

    unsafe impl NSObjectProtocol for FrameOutput {}

    unsafe impl SCStreamOutput for FrameOutput {
        #[unsafe(method(stream:didOutputSampleBuffer:ofType:))]
        unsafe fn stream_did_output(&self, _stream: &SCStream, sample: &CMSampleBuffer, kind: SCStreamOutputType) {
            if kind != SCStreamOutputType::Screen {
                return;
            }
            if let Some(frame) = read_frame(sample, self.ivars().window) {
                (self.ivars().sink)(frame);
            }
        }
    }
);

impl FrameOutput {
    fn new(window: u32, sink: Sink) -> Retained<Self> {
        let this = Self::alloc().set_ivars(OutputIvars { window, sink });
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

pub struct Stream {
    stream: Sendable<Retained<SCStream>>,
    _output: Sendable<Retained<FrameOutput>>,
    _queue: Sendable<DispatchRetained<DispatchQueue>>,
}

impl Stream {
    pub fn start(window: u32, interval: Duration, sink: Sink) -> Result<Stream, CaptureError> {
        let content = shareable_content()?;
        // SAFETY: property reads and initializers on live ScreenCaptureKit
        // objects, used as Apple documents them.
        unsafe {
            let found = content
                .windows()
                .iter()
                .find(|w| w.windowID() == window)
                .ok_or(CaptureError::WindowGone(window))?;
            let filter = SCContentFilter::initWithDesktopIndependentWindow(SCContentFilter::alloc(), &found);
            let info = SCShareableContent::infoForFilter(&filter);
            let scale = f64::from(info.pointPixelScale());
            let rect = info.contentRect();
            let (mut width, mut height) = (rect.size.width * scale, rect.size.height * scale);
            if width > MAX_WIDTH {
                height *= MAX_WIDTH / width;
                width = MAX_WIDTH;
            }
            let config = SCStreamConfiguration::new();
            config.setWidth(width.round().max(1.0) as usize);
            config.setHeight(height.round().max(1.0) as usize);
            let millis = i64::try_from(interval.as_millis()).unwrap_or(i64::MAX).max(1);
            config.setMinimumFrameInterval(CMTime::new(millis, 1000));
            config.setPixelFormat(kCVPixelFormatType_32BGRA);
            config.setShowsCursor(false);
            config.setCapturesAudio(false);
            config.setQueueDepth(3);

            let output = FrameOutput::new(window, sink);
            let stream = SCStream::initWithFilter_configuration_delegate(SCStream::alloc(), &filter, &config, None);
            // Frames are read at the utility level, below the game.
            let queue = DispatchQueue::new_with_target(
                "io.github.pasouvenla.krozties.capture",
                None,
                Some(&DispatchQueue::global_queue(GlobalQueueIdentifier::QualityOfService(DispatchQoS::Utility))),
            );
            stream
                .addStreamOutput_type_sampleHandlerQueue_error(
                    ProtocolObject::from_ref(&*output),
                    SCStreamOutputType::Screen,
                    Some(&queue),
                )
                .map_err(|e| error_from(&e))?;

            let (tx, rx) = mpsc::channel();
            let started = RcBlock::new(move |error: *mut NSError| {
                let _ = tx.send(Retained::retain(error).map(|e| error_from(&e)));
            });
            stream.startCaptureWithCompletionHandler(Some(&started));
            match rx.recv_timeout(TIMEOUT) {
                Ok(None) => Ok(Stream { stream: Sendable(stream), _output: Sendable(output), _queue: Sendable(queue) }),
                Ok(Some(e)) => Err(e),
                Err(_) => Err(CaptureError::Failed("ScreenCaptureKit did not start the capture".into())),
            }
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: stopping a live stream; nothing waits for the answer.
        unsafe { self.stream.0.stopCaptureWithCompletionHandler(None) };
    }
}
