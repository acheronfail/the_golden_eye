use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Result;

use crate::app::AppState;
use crate::diagnostics::DumpStartError;

#[derive(serde::Deserialize)]
pub struct FrameDumpParams {
    enabled: bool,
    /// Source to dump; required when enabling. Ignored when disabling.
    #[serde(default)]
    source: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameDumpResponse {
    frame_dump_enabled: bool,
}

/// Toggles the transient developer frame dump. Any existing dump is stopped
/// first, so this also handles switching to a different source.
#[axum::debug_handler]
pub async fn handle_frame_dump(
    State(state): State<AppState>,
    Json(params): Json<FrameDumpParams>,
) -> Result<Json<FrameDumpResponse>> {
    state.frame_dump.stop().await;
    if params.enabled {
        let source = params.source.ok_or((StatusCode::BAD_REQUEST, "source is required to enable the frame dump"))?;
        state.frame_dump.start(source).map_err(dump_error_response)?;
    }
    Ok(Json(FrameDumpResponse { frame_dump_enabled: params.enabled }))
}

fn dump_error_response(error: DumpStartError) -> (StatusCode, &'static str) {
    match error {
        DumpStartError::InvalidSource => (StatusCode::BAD_REQUEST, "source name contains a null byte"),
        DumpStartError::CaptureUnavailable => (StatusCode::INTERNAL_SERVER_ERROR, "failed to create capture context"),
        DumpStartError::WorkerUnavailable => (StatusCode::INTERNAL_SERVER_ERROR, "failed to spawn frame dump thread"),
    }
}
