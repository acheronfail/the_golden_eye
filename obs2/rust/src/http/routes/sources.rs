use axum::Json;
use axum::response::{IntoResponse, Result};
use serde::Serialize;

#[derive(Serialize)]
pub struct Source {
    name: String,
    id: String,
}

const SOURCE_NAMES_BUFFER_SIZE: usize = 4096;

pub async fn handler() -> Result<impl IntoResponse> {
    let mut buffer = [0i8; SOURCE_NAMES_BUFFER_SIZE];
    unsafe {
        crate::obs_ffi::ge_obs_collect_source_names(buffer.as_mut_ptr(), SOURCE_NAMES_BUFFER_SIZE);
    }

    let sources: Vec<Source> = unsafe { std::ffi::CStr::from_ptr(buffer.as_ptr()) }
        .to_str()
        .unwrap_or_default()
        .split('\n')
        .filter(|s| !s.is_empty())
        .filter_map(|line| {
            let (name, id) = line.split_once('\t')?;
            Some(Source { name: name.to_owned(), id: id.to_owned() })
        })
        .collect();

    Ok(Json(sources))
}
