//! The Windows backend: the game's windows as the system lists them, their
//! frames through Windows.Graphics.Capture, their text through
//! Windows.Media.Ocr. None needs a permission. Where the system allows it,
//! Windows draws no border around a window being captured.
//!
//! The game's processes are known by the name of their executable, from the
//! system's list of processes: none of them is ever opened.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::core::{Interface, BOOL};
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Graphics::SizeInt32;
use windows::Media::Ocr::OcrEngine;
use windows::Storage::Streams::DataWriter;
use windows::Win32::Foundation::{CloseHandle, HMODULE, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::{
    D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D, D3D11_CPU_ACCESS_READ,
    D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SDK_VERSION,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
};

use crate::place::Line;
use crate::text::Region;
use crate::{CaptureError, Frame, GameWindow, Rect, Sink};

/// The executable of the Dofus 3 client.
const GAME_EXE: &str = "Dofus.exe";
/// A smaller window is a launcher, a menu or a tooltip, not the game.
const MIN_SIDE: f64 = 200.0;

fn failed(e: windows::core::Error) -> CaptureError {
    CaptureError::Failed(e.message())
}

/// Window handles fit in 32 bits, on 64-bit Windows too.
fn handle(id: u32) -> HWND {
    HWND(id as usize as *mut core::ffi::c_void)
}

fn id_of(window: HWND) -> u32 {
    window.0 as usize as u32
}

/// The ids of the game's processes, from the system's list of processes.
fn game_processes() -> Vec<u32> {
    let mut ids = Vec::new();
    // SAFETY: the snapshot is read entry after entry into a structure whose
    // size is given, then closed.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return ids;
        };
        let mut entry = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry).is_ok();
        while more {
            let length = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..length]).eq_ignore_ascii_case(GAME_EXE) {
                ids.push(entry.th32ProcessID);
            }
            more = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    ids
}

/// Gathers every top-level window into the list `list` points to.
unsafe extern "system" fn gather(window: HWND, list: LPARAM) -> BOOL {
    // SAFETY: `game_windows` passes a pointer to its list, alive for the call.
    let list = unsafe { &mut *(list.0 as *mut Vec<HWND>) };
    list.push(window);
    true.into()
}

pub fn game_windows() -> Result<Vec<GameWindow>, CaptureError> {
    let processes = game_processes();
    if processes.is_empty() {
        return Ok(Vec::new());
    }
    let mut all: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes into the list, which outlives the call.
    unsafe { EnumWindows(Some(gather), LPARAM(&mut all as *mut Vec<HWND> as isize)) }.map_err(failed)?;
    let mut windows = Vec::new();
    for window in all {
        // SAFETY: queries of a window handle the system just listed; the
        // buffers are sized for what they receive.
        unsafe {
            if !IsWindowVisible(window).as_bool() || IsIconic(window).as_bool() {
                continue;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if !processes.contains(&pid) {
                continue;
            }
            // A cloaked window is on another virtual desktop.
            let mut cloaked = 0u32;
            let size = std::mem::size_of::<u32>() as u32;
            if DwmGetWindowAttribute(window, DWMWA_CLOAKED, (&mut cloaked as *mut u32).cast(), size).is_ok() && cloaked != 0 {
                continue;
            }
            let mut bounds = RECT::default();
            let size = std::mem::size_of::<RECT>() as u32;
            if DwmGetWindowAttribute(window, DWMWA_EXTENDED_FRAME_BOUNDS, (&mut bounds as *mut RECT).cast(), size).is_err() {
                continue;
            }
            // In points, as on macOS: pixels at the window's own scale.
            let scale = f64::from(GetDpiForWindow(window).max(96)) / 96.0;
            let frame = Rect {
                x: f64::from(bounds.left) / scale,
                y: f64::from(bounds.top) / scale,
                width: f64::from(bounds.right - bounds.left) / scale,
                height: f64::from(bounds.bottom - bounds.top) / scale,
            };
            if frame.width < MIN_SIDE || frame.height < MIN_SIDE {
                continue;
            }
            let mut title = [0u16; 256];
            let length = GetWindowTextW(window, &mut title).max(0) as usize;
            windows.push(GameWindow {
                id: id_of(window),
                pid: pid as i32,
                title: String::from_utf16_lossy(&title[..length]),
                frame,
            });
        }
    }
    Ok(windows)
}

pub fn active_game_window(windows: &[GameWindow]) -> Option<u32> {
    // SAFETY: no argument; the handle is only compared.
    let front = id_of(unsafe { GetForegroundWindow() });
    windows.iter().any(|w| w.id == front).then_some(front)
}

/// What the capture handler needs between two frames.
struct Capture {
    window: u32,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    direct3d: IDirect3DDevice,
    size: SizeInt32,
    interval: Duration,
    last: Option<Instant>,
    sink: Sink,
}

// SAFETY: the Direct3D device is free-threaded; its immediate context is
// only used under the mutex that holds this, one frame at a time.
unsafe impl Send for Capture {}

impl Capture {
    /// Reads the next frame of the pool into memory and hands it over, at
    /// most once per interval.
    fn on_frame(&mut self, pool: &Direct3D11CaptureFramePool) -> windows::core::Result<()> {
        let frame = pool.TryGetNextFrame()?;
        let content = frame.ContentSize()?;
        if content.Width != self.size.Width || content.Height != self.size.Height {
            // The window changed size: the pool follows, from the next frame.
            self.size = content;
            return pool.Recreate(&self.direct3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, content);
        }
        let now = Instant::now();
        if self.last.is_some_and(|last| now.duration_since(last) < self.interval) {
            return Ok(());
        }
        self.last = Some(now);
        let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
        // SAFETY: the texture of the frame is copied into a staging texture of
        // its own description, readable by the processor, then mapped, read
        // row by row within its pitch, and unmapped.
        unsafe {
            let texture: ID3D11Texture2D = access.GetInterface()?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            desc.Usage = D3D11_USAGE_STAGING;
            desc.BindFlags = 0;
            desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
            desc.MiscFlags = 0;
            let mut staging: Option<ID3D11Texture2D> = None;
            self.device.CreateTexture2D(&desc, None, Some(&mut staging))?;
            let Some(staging) = staging else {
                return Ok(());
            };
            self.context.CopyResource(&staging, &texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let width = (content.Width.max(0) as u32).min(desc.Width) as usize;
            let height = (content.Height.max(0) as u32).min(desc.Height) as usize;
            let mut bgra = Vec::with_capacity(width * height * 4);
            for y in 0..height {
                let row = (mapped.pData as *const u8).add(y * mapped.RowPitch as usize);
                bgra.extend_from_slice(std::slice::from_raw_parts(row, width * 4));
            }
            self.context.Unmap(&staging, 0);
            (self.sink)(Frame { window: self.window, width: width as u32, height: height as u32, bgra });
        }
        Ok(())
    }
}

/// A running capture of one game window.
pub struct Stream {
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
}

impl Stream {
    pub fn start(window: u32, interval: Duration, sink: Sink) -> Result<Stream, CaptureError> {
        let missing = |what: &str| CaptureError::Failed(format!("no {what}"));
        // SAFETY: a hardware Direct3D device with its immediate context,
        // wrapped for Windows.Graphics.Capture; the window handle comes from
        // the system's list.
        unsafe {
            let (mut device, mut context) = (None::<ID3D11Device>, None::<ID3D11DeviceContext>);
            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                None,
                D3D11_SDK_VERSION,
                Some(&mut device),
                None,
                Some(&mut context),
            )
            .map_err(failed)?;
            let device = device.ok_or_else(|| missing("Direct3D device"))?;
            let context = context.ok_or_else(|| missing("Direct3D context"))?;
            let dxgi: IDXGIDevice = device.cast().map_err(failed)?;
            let direct3d: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi).map_err(failed)?.cast().map_err(failed)?;
            let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>().map_err(failed)?;
            let item: GraphicsCaptureItem =
                interop.CreateForWindow(handle(window)).map_err(|_| CaptureError::WindowGone(window))?;
            let size = item.Size().map_err(failed)?;
            let pool =
                Direct3D11CaptureFramePool::CreateFreeThreaded(&direct3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2, size)
                    .map_err(failed)?;
            let session = pool.CreateCaptureSession(&item).map_err(failed)?;
            // Neither exists on every version of Windows: without, the cursor
            // shows, or the border.
            let _ = session.SetIsCursorCaptureEnabled(false);
            let _ = session.SetIsBorderRequired(false);
            let capture =
                Mutex::new(Capture { window, device, context, direct3d, size, interval, last: None, sink });
            pool.FrameArrived(&TypedEventHandler::new(move |pool: windows::core::Ref<'_, Direct3D11CaptureFramePool>, _| {
                if let (Ok(pool), Ok(mut capture)) = (pool.ok(), capture.lock()) {
                    // A frame that fails is skipped: the next one comes.
                    let _ = capture.on_frame(pool);
                }
                Ok(())
            }))
            .map_err(failed)?;
            session.StartCapture().map_err(failed)?;
            Ok(Stream { session, pool })
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

/// The lines of text in a region of a frame, read by Windows.Media.Ocr in the
/// languages of the player's profile.
pub fn lines(frame: &Frame, region: Region) -> Result<Vec<Line>, CaptureError> {
    let (left, top, width, height) = region.pixels(frame);
    if width == 0 || height == 0 {
        return Err(CaptureError::Failed("text recognition: empty region".into()));
    }
    let row = frame.width as usize * 4;
    let mut bytes = Vec::with_capacity(width * height * 4);
    for y in top..top + height {
        let start = y * row + left * 4;
        bytes.extend_from_slice(frame.bgra.get(start..start + width * 4).ok_or_else(|| CaptureError::Failed("short frame".into()))?);
    }
    let writer = DataWriter::new().map_err(failed)?;
    writer.WriteBytes(&bytes).map_err(failed)?;
    let buffer = writer.DetachBuffer().map_err(failed)?;
    let bitmap =
        SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Bgra8, width as i32, height as i32).map_err(failed)?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().map_err(failed)?;
    let result = engine.RecognizeAsync(&bitmap).map_err(failed)?.get().map_err(failed)?;
    let mut lines = Vec::new();
    for line in result.Lines().map_err(failed)? {
        let text = line.Text().map_err(failed)?.to_string();
        // The line's bounds: those of its words together.
        let mut bounds: Option<(f32, f32, f32, f32)> = None;
        for word in line.Words().map_err(failed)? {
            let r = word.BoundingRect().map_err(failed)?;
            let (x0, y0, x1, y1) = (r.X, r.Y, r.X + r.Width, r.Y + r.Height);
            bounds = Some(match bounds {
                None => (x0, y0, x1, y1),
                Some((a, b, c, d)) => (a.min(x0), b.min(y0), c.max(x1), d.max(y1)),
            });
        }
        let (x0, y0, x1, y1) = bounds.unwrap_or_default();
        // The recognition says nothing of how sure it is.
        lines.push(Line { text, confidence: 1.0, bounds: (left as f32 + x0, top as f32 + y0, x1 - x0, y1 - y0) });
    }
    Ok(lines)
}
