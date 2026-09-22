//! Developer capture diagnostics; these do not own the gameplay monitor's state.
mod frame_dump;
pub(crate) mod screenshot;
pub(crate) use frame_dump::{DumpStartError, FrameDumper};

pub(crate) mod matching;
