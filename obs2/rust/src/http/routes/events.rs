use std::time::Duration;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use tokio::sync::{broadcast, watch};

use crate::http::{AppEvent, AppState};

const UPDATE_APPLIED_NOTICE_WINDOW: Duration = Duration::from_secs(60);

/// Upgrades the connection to the app-wide event stream. The complete retained
/// snapshot is sent on connect and after every retained-state change; one-off
/// events are forwarded as they occur.
pub async fn handle_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut snapshots = state.snapshot.subscribe();
    let mut events = state.event_tx.subscribe();

    let version = AppEvent::Version { build_id: crate::http::routes::index::BUILD_ID.clone() };
    if send_event(&mut socket, &version).await.is_err() {
        return;
    }

    if send_current_snapshot(&mut socket, &mut snapshots).await.is_err() {
        return;
    }

    // A one-off "plugin updated" notice for clients connecting shortly after
    // an applied update. The grace period prevents stale notices in later tabs.
    if state.reloaded_at.is_some_and(|when| when.elapsed() < UPDATE_APPLIED_NOTICE_WINDOW) {
        let settings = state.settings.get();
        let release_url = if settings.last_known_update_version.as_deref() == Some(crate::PLUGIN_VERSION) {
            settings.last_known_update_release_url
        } else {
            None
        };
        let applied = AppEvent::UpdateApplied { version: crate::PLUGIN_VERSION.to_owned(), release_url };
        if send_event(&mut socket, &applied).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            changed = snapshots.changed() => {
                if changed.is_err() {
                    break;
                }
                if send_current_snapshot(&mut socket, &mut snapshots).await.is_err() {
                    break;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if send_event(&mut socket, &event).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(skipped, "app event client lagged; retained state will recover on the next snapshot");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(message)) => {
                        tracing::warn!(kind = message_kind(&message), "ignoring unexpected app event client message");
                    }
                }
            }
        }
    }
}

fn message_kind(message: &Message) -> &'static str {
    match message {
        Message::Text(_) => "text",
        Message::Binary(_) => "binary",
        Message::Ping(_) => "ping",
        Message::Pong(_) => "pong",
        Message::Close(_) => "close",
    }
}

async fn send_event(socket: &mut WebSocket, event: &AppEvent) -> Result<(), axum::Error> {
    match serde_json::to_string(event) {
        Ok(text) => socket.send(Message::Text(text.into())).await?,
        Err(err) => tracing::warn!(%err, "failed to serialize app event"),
    }
    Ok(())
}

async fn send_current_snapshot(
    socket: &mut WebSocket,
    rx: &mut watch::Receiver<crate::http::AppSnapshot>,
) -> Result<(), axum::Error> {
    let state = Box::new(rx.borrow_and_update().clone());
    send_event(socket, &AppEvent::Snapshot { state }).await
}
