use std::ffi::c_void;

use super::raw::{self, ObsTaskType};

type UiTask = Box<dyn FnOnce() + Send>;

pub fn queue_ui_task(task: impl FnOnce() + Send + 'static) -> anyhow::Result<()> {
    static PINNED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !PINNED.get_or_init(|| unsafe { raw::ge_obs_pin_core_for_ui() }) {
        anyhow::bail!("could not retain the core for native UI callbacks");
    }
    let task: Box<UiTask> = Box::new(Box::new(task));
    unsafe { raw::obs_queue_task(ObsTaskType::Ui, run_ui_task, Box::into_raw(task).cast(), false) };
    Ok(())
}

unsafe extern "C" fn run_ui_task(param: *mut c_void) {
    // SAFETY: queue_ui_task transfers one Box<UiTask> to this one-shot callback.
    let task = unsafe { Box::from_raw(param.cast::<UiTask>()) };
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(*task)).is_err() {
        tracing::error!("OBS UI task panicked");
    }
}
