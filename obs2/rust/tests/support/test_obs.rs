use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

type FrameCallback = unsafe extern "C" fn(*mut c_void, u32, u32);
type ObsTask = unsafe extern "C" fn(*mut c_void);

#[derive(Clone)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Calls {
    pub queue_task: usize,
    pub recording_start: usize,
    pub recording_stop: usize,
    pub replay_start: usize,
    pub replay_stop: usize,
    pub replay_save: usize,
    pub replay_active: usize,
    pub replay_enabled: usize,
    pub replay_available: usize,
    pub replay_max_seconds: usize,
    pub replay_output_directory: usize,
    pub source_names: usize,
    pub user_config: usize,
    pub config_get_string: usize,
    pub config_set_string: usize,
    pub capture_create: usize,
    pub capture_get_frame: usize,
    pub capture_destroy: usize,
    pub frame_callback_register: usize,
    pub frame_callback_unregister: usize,
    pub dock_config_save: usize,
    pub core_trigger_reload: usize,
}

impl Calls {
    pub fn runtime_frontend_queries(&self) -> usize {
        self.replay_active
            + self.replay_enabled
            + self.replay_available
            + self.replay_max_seconds
            + self.replay_output_directory
            + self.source_names
    }

    pub fn dock_config_queries(&self) -> usize {
        self.user_config + self.config_get_string + self.config_set_string + self.dock_config_save
    }
}

pub struct Config {
    pub data_path: PathBuf,
    pub replay_output_directory: PathBuf,
    pub replay_fixture: PathBuf,
    pub fps: f64,
    pub replay_enabled: bool,
    pub replay_available: bool,
    pub replay_active: bool,
    pub replay_max_seconds: i64,
    pub replay_stop_delay: Duration,
    /// Delay between a save request and its saved event firing. Zero fires
    /// synchronously (default); nonzero fires from a background thread, modelling
    /// OBS's asynchronous save so overlapping saves can be exercised.
    pub replay_save_delay: Duration,
    pub sources: Vec<(String, String)>,
}

struct Callback {
    cb: FrameCallback,
    param: usize,
}

struct State {
    config: Config,
    calls: Calls,
    frames: VecDeque<Frame>,
    current_frame: Option<Frame>,
    callback: Option<Callback>,
    dock_json: CString,
    live_dock_json: CString,
    replay_serial: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            config: Config {
                data_path: PathBuf::new(),
                replay_output_directory: PathBuf::new(),
                replay_fixture: PathBuf::new(),
                fps: 60.0,
                replay_enabled: true,
                replay_available: true,
                replay_active: false,
                replay_max_seconds: 60,
                replay_stop_delay: Duration::ZERO,
                replay_save_delay: Duration::ZERO,
                sources: Vec::new(),
            },
            calls: Calls::default(),
            frames: VecDeque::new(),
            current_frame: None,
            callback: None,
            dock_json: CString::new("[]").unwrap(),
            live_dock_json: CString::new("[]").unwrap(),
            replay_serial: 0,
        }
    }
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(|| Mutex::new(State::default()));

pub struct TestObs;

impl TestObs {
    pub fn install(config: Config) -> Self {
        *STATE.lock().unwrap() = State { config, ..State::default() };
        TestObs
    }

    pub fn calls(&self) -> Calls {
        STATE.lock().unwrap().calls.clone()
    }

    pub fn replay_active(&self) -> bool {
        STATE.lock().unwrap().config.replay_active
    }

    pub fn set_frame(&self, frame: Frame) {
        STATE.lock().unwrap().current_frame = Some(frame);
    }

    pub fn set_sources(&self, sources: Vec<(String, String)>) {
        STATE.lock().unwrap().config.sources = sources;
    }

    pub fn set_replay_save_delay(&self, delay: Duration) {
        STATE.lock().unwrap().config.replay_save_delay = delay;
    }

    /// Simulate the user saving the replay buffer themselves (OBS hotkey/button):
    /// OBS writes a file and fires the saved event without the plugin asking, so no
    /// `replay_save` is counted. The plugin must leave it alone. Returns the path.
    pub fn user_replay_save(&self) -> PathBuf {
        let path = write_replay_file("user-replay");
        fire_replay_saved(&path);
        path
    }

    pub fn render(&self, frame: Frame) {
        let callback = {
            let mut state = STATE.lock().unwrap();
            state.frames.push_back(frame);
            state.callback.as_ref().map(|callback| (callback.cb, callback.param))
        };
        if let Some((cb, param)) = callback {
            // SAFETY: the plugin owns param while the callback is registered.
            unsafe { cb(param as *mut c_void, 640, 480) };
        }
    }

    pub fn dock_json(&self) -> String {
        STATE.lock().unwrap().dock_json.to_string_lossy().into_owned()
    }

    pub fn live_dock_json(&self) -> String {
        STATE.lock().unwrap().live_dock_json.to_string_lossy().into_owned()
    }

    pub fn simulate_obs_load_extra_browser_docks(&self) {
        let mut state = STATE.lock().unwrap();
        state.live_dock_json = CString::new(state.dock_json.to_bytes()).unwrap();
    }

    pub fn simulate_obs_save_extra_browser_docks(&self) {
        let mut state = STATE.lock().unwrap();
        state.dock_json = CString::new(state.live_dock_json.to_bytes()).unwrap();
    }
}

fn copy_to_c(value: &Path, buffer: *mut c_char, buffer_size: usize) -> bool {
    let bytes = value.to_string_lossy();
    if buffer.is_null() || bytes.len() + 1 > buffer_size {
        return false;
    }
    // SAFETY: the caller supplied a buffer large enough for bytes and NUL.
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), bytes.len());
        *buffer.add(bytes.len()) = 0;
    }
    true
}

fn malloc_frame(frame: &Frame, out_width: *mut u32, out_height: *mut u32) -> *mut u8 {
    // SAFETY: OBS bridge contracts require valid output pointers.
    unsafe {
        *out_width = frame.width;
        *out_height = frame.height;
        let output = libc::malloc(frame.bgra.len()).cast::<u8>();
        if !output.is_null() {
            ptr::copy_nonoverlapping(frame.bgra.as_ptr(), output, frame.bgra.len());
        }
        output
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn obs_queue_task(_kind: c_int, task: ObsTask, param: *mut c_void, _wait: bool) {
    STATE.lock().unwrap().calls.queue_task += 1;
    // SAFETY: this test host executes UI work synchronously on its host thread.
    unsafe { task(param) };
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_recording_start() {
    STATE.lock().unwrap().calls.recording_start += 1;
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_recording_stop() {
    STATE.lock().unwrap().calls.recording_stop += 1;
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_replay_buffer_active() -> bool {
    let mut state = STATE.lock().unwrap();
    state.calls.replay_active += 1;
    state.config.replay_active
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_replay_buffer_start() {
    {
        let mut state = STATE.lock().unwrap();
        state.calls.replay_start += 1;
        state.config.replay_active = true;
    }
    ge_rust::ge_replay_buffer_starting();
    ge_rust::ge_replay_buffer_started();
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_replay_buffer_stop() {
    let delay = {
        let mut state = STATE.lock().unwrap();
        state.calls.replay_stop += 1;
        state.config.replay_stop_delay
    };
    ge_rust::ge_replay_buffer_stopping();
    if delay.is_zero() {
        finish_replay_buffer_stop();
    } else {
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            finish_replay_buffer_stop();
        });
    }
}

fn finish_replay_buffer_stop() {
    STATE.lock().unwrap().config.replay_active = false;
    ge_rust::ge_replay_buffer_stopped();
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_replay_buffer_save() {
    let delay = {
        let mut state = STATE.lock().unwrap();
        state.calls.replay_save += 1;
        state.config.replay_save_delay
    };
    let path = write_replay_file("obs-replay");
    if delay.is_zero() {
        fire_replay_saved(&path);
    } else {
        // Model OBS's asynchronous save: the request returns immediately and the
        // saved event fires later, opening the window an overlapping save needs.
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            fire_replay_saved(&path);
        });
    }
}

/// Write a copy of the replay fixture into the output directory with a unique
/// serialized name, returning its path. Shared by plugin- and user-initiated saves.
fn write_replay_file(prefix: &str) -> PathBuf {
    let mut state = STATE.lock().unwrap();
    state.replay_serial += 1;
    let path = state.config.replay_output_directory.join(format!("{prefix}-{}.mp4", state.replay_serial));
    std::fs::create_dir_all(&state.config.replay_output_directory).unwrap();
    std::fs::copy(&state.config.replay_fixture, &path).unwrap();
    path
}

fn fire_replay_saved(path: &Path) {
    let saved = CString::new(path.to_string_lossy().as_bytes()).unwrap();
    // SAFETY: saved remains alive for the duration of the callback.
    unsafe { ge_rust::ge_replay_buffer_saved(saved.as_ptr()) };
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_replay_buffer_enabled() -> bool {
    let mut state = STATE.lock().unwrap();
    state.calls.replay_enabled += 1;
    state.config.replay_enabled
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_replay_buffer_available() -> bool {
    let mut state = STATE.lock().unwrap();
    state.calls.replay_available += 1;
    state.config.replay_available
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_replay_buffer_max_seconds() -> i64 {
    let mut state = STATE.lock().unwrap();
    state.calls.replay_max_seconds += 1;
    state.config.replay_max_seconds
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_replay_buffer_output_directory(buffer: *mut c_char, size: usize) -> bool {
    let mut state = STATE.lock().unwrap();
    state.calls.replay_output_directory += 1;
    let path = state.config.replay_output_directory.clone();
    drop(state);
    copy_to_c(&path, buffer, size)
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_module_data_path(buffer: *mut c_char, size: usize) -> bool {
    copy_to_c(&STATE.lock().unwrap().config.data_path, buffer, size)
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_video_fps() -> f64 {
    STATE.lock().unwrap().config.fps
}

// `c_int` stands in for the ABI-identical `GeLogLevel` (a repr(C) fieldless enum
// is int-sized); this crate can't see that crate-private type. Print via stderr
// so `--nocapture` exposes Rust tracing output in integration tests.
#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_blog(_level: c_int, msg: *const c_char) {
    if msg.is_null() {
        return;
    }

    // SAFETY: `ge_rust` passes a valid NUL-terminated string for this call.
    let msg = unsafe { std::ffi::CStr::from_ptr(msg) };
    eprintln!("{}", msg.to_string_lossy());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_obs_collect_source_names(buffer: *mut c_char, size: usize) {
    let names = {
        let mut state = STATE.lock().unwrap();
        state.calls.source_names += 1;
        state.config.sources.iter().map(|(name, id)| format!("{name}\t{id}")).collect::<Vec<_>>().join("\n")
    };
    if names.len() < size {
        // SAFETY: checked against the supplied size.
        unsafe {
            ptr::copy_nonoverlapping(names.as_ptr(), buffer.cast(), names.len());
            *buffer.add(names.len()) = 0;
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_obs_get_source_frame(
    _source: *const c_char,
    out_width: *mut u32,
    out_height: *mut u32,
) -> *mut u8 {
    let frame = STATE.lock().unwrap().current_frame.clone();
    frame.as_ref().map_or(ptr::null_mut(), |frame| malloc_frame(frame, out_width, out_height))
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_capture_create(_double_buffered: bool) -> *mut c_void {
    STATE.lock().unwrap().calls.capture_create += 1;
    Box::into_raw(Box::new(0_u8)).cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_capture_get_frame(
    _ctx: *mut c_void,
    _source: *const c_char,
    _max_height: u32,
    _region: *const c_void,
    out_width: *mut u32,
    out_height: *mut u32,
) -> *mut u8 {
    let frame = {
        let mut state = STATE.lock().unwrap();
        state.calls.capture_get_frame += 1;
        state.frames.pop_front()
    };
    frame.as_ref().map_or(ptr::null_mut(), |frame| malloc_frame(frame, out_width, out_height))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ge_capture_destroy(ctx: *mut c_void) {
    STATE.lock().unwrap().calls.capture_destroy += 1;
    if !ctx.is_null() {
        // SAFETY: allocated by ge_capture_create and destroyed exactly once.
        drop(unsafe { Box::from_raw(ctx.cast::<u8>()) });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_register_frame_callback(cb: FrameCallback, param: *mut c_void) {
    let mut state = STATE.lock().unwrap();
    state.calls.frame_callback_register += 1;
    state.callback = Some(Callback { cb, param: param as usize });
}

#[unsafe(no_mangle)]
pub extern "C" fn ge_obs_unregister_frame_callback(_cb: FrameCallback, _param: *mut c_void) {
    let mut state = STATE.lock().unwrap();
    state.calls.frame_callback_unregister += 1;
    state.callback = None;
}

#[unsafe(no_mangle)]
pub extern "C" fn obs_frontend_get_user_config() -> *mut c_void {
    STATE.lock().unwrap().calls.user_config += 1;
    ptr::dangling_mut::<u8>().cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn config_get_string(
    _config: *mut c_void,
    _section: *const c_char,
    _name: *const c_char,
) -> *const c_char {
    let mut state = STATE.lock().unwrap();
    state.calls.config_get_string += 1;
    state.dock_json.as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn config_set_string(
    _config: *mut c_void,
    _section: *const c_char,
    _name: *const c_char,
    value: *const c_char,
) {
    // SAFETY: OBS config API supplies a valid string for the duration of the call.
    let value = unsafe { CStr::from_ptr(value) };
    let mut state = STATE.lock().unwrap();
    state.calls.config_set_string += 1;
    state.dock_json = CString::new(value.to_bytes()).unwrap();
}

#[unsafe(no_mangle)]
pub extern "C" fn config_save_safe(_config: *mut c_void, _temp: *const c_char, _backup: *const c_char) -> c_int {
    STATE.lock().unwrap().calls.dock_config_save += 1;
    0
}

/// Stand-in for the real `ge_core_trigger_reload` (implemented in core.c and
/// wired to the shim's reload worker thread) -- these integration tests link
/// the Rust crate directly, without any C bridge, so there's no real shim to
/// wake. Just records that it was called; the actual dlopen/rename/rollback
/// mechanics have their own dedicated tests next to shim/reload.c.
#[unsafe(no_mangle)]
pub extern "C" fn ge_core_trigger_reload() {
    STATE.lock().unwrap().calls.core_trigger_reload += 1;
}
