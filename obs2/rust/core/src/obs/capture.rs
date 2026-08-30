use std::ffi::{CStr, c_void};
use std::ptr::NonNull;

use super::raw::{self, GeCaptureCtx};
pub use super::raw::{GeCaptureRegion, GeCaptureTimings};

/// One BGRA source frame allocated by the C bridge.
pub struct OwnedBgraFrame {
    ptr: NonNull<u8>,
    width: u32,
    height: u32,
    len: usize,
}

impl OwnedBgraFrame {
    fn from_raw(ptr: *mut u8, width: u32, height: u32) -> Option<Self> {
        let ptr = NonNull::new(ptr)?;
        let len = (width as usize).checked_mul(height as usize).and_then(|pixels| pixels.checked_mul(4));
        let Some(len) = len.filter(|len| *len > 0) else {
            // SAFETY: the non-null allocation came from the C bridge.
            unsafe { raw::free(ptr.as_ptr().cast()) };
            return None;
        };
        Some(Self { ptr, width, height, len })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn bytes(&self) -> &[u8] {
        // SAFETY: ptr/len describe this value's exclusively owned allocation.
        unsafe { std::slice::from_raw_parts(self.ptr.as_ptr(), self.len) }
    }
}

// SAFETY: the frame exclusively owns its allocation and exposes immutable bytes.
unsafe impl Send for OwnedBgraFrame {}

impl Drop for OwnedBgraFrame {
    fn drop(&mut self) {
        // SAFETY: this value exclusively owns the C bridge allocation.
        unsafe { raw::free(self.ptr.as_ptr().cast()) };
    }
}

pub fn capture_source_frame(source_name: &CStr) -> Option<OwnedBgraFrame> {
    let mut width = 0;
    let mut height = 0;
    let ptr = unsafe { raw::ge_obs_get_source_frame(source_name.as_ptr(), &mut width, &mut height) };
    OwnedBgraFrame::from_raw(ptr, width, height)
}

/// Reusable OBS render/stage surfaces for repeated source capture.
pub struct CaptureContext {
    ptr: NonNull<GeCaptureCtx>,
}

// SAFETY: registration serializes context use with teardown before it moves.
unsafe impl Send for CaptureContext {}

impl CaptureContext {
    pub fn new(double_buffered: bool) -> Option<Self> {
        NonNull::new(unsafe { raw::ge_capture_create(double_buffered) }).map(|ptr| Self { ptr })
    }

    pub fn capture(
        &mut self,
        source_name: &CStr,
        max_height: u32,
        region: Option<&GeCaptureRegion>,
        timings: Option<&mut GeCaptureTimings>,
    ) -> Option<OwnedBgraFrame> {
        let mut width = 0;
        let mut height = 0;
        let region = region.map_or(std::ptr::null(), |region| region as *const _);
        let timings = timings.map_or(std::ptr::null_mut(), |timings| timings as *mut _);
        let ptr = unsafe {
            raw::ge_capture_get_frame(
                self.ptr.as_ptr(),
                source_name.as_ptr(),
                max_height,
                region,
                &mut width,
                &mut height,
                timings,
            )
        };
        OwnedBgraFrame::from_raw(ptr, width, height)
    }
}

impl Drop for CaptureContext {
    fn drop(&mut self) {
        unsafe { raw::ge_capture_destroy(self.ptr.as_ptr()) };
    }
}

pub trait RenderCallback: Send + 'static {
    fn render_frame(&mut self, canvas_width: u32, canvas_height: u32);
}

/// Owns callback state and reclaims it only after OBS unregisters.
pub struct RegisteredRenderCallback<T: RenderCallback> {
    state: NonNull<T>,
}

// SAFETY: OBS serializes callback execution with unregister before reclaiming T.
unsafe impl<T: RenderCallback> Send for RegisteredRenderCallback<T> {}

impl<T: RenderCallback> RegisteredRenderCallback<T> {
    pub fn register(state: T) -> Self {
        let state = NonNull::from(Box::leak(Box::new(state)));
        unsafe { raw::ge_obs_register_frame_callback(render_callback::<T>, state.as_ptr().cast()) };
        Self { state }
    }
}

impl<T: RenderCallback> Drop for RegisteredRenderCallback<T> {
    fn drop(&mut self) {
        unsafe {
            raw::ge_obs_unregister_frame_callback(render_callback::<T>, self.state.as_ptr().cast());
            drop(Box::from_raw(self.state.as_ptr()));
        }
    }
}

unsafe extern "C" fn render_callback<T: RenderCallback>(param: *mut c_void, cx: u32, cy: u32) {
    // SAFETY: registration owns a live T until unregister fences this callback.
    let state = unsafe { &mut *param.cast::<T>() };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| state.render_frame(cx, cy))).is_err() {
        tracing::error!("OBS render callback panicked");
    }
}
