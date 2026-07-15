use std::time::Duration;

use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use tokio::sync::{broadcast, watch};

use crate::http::{AppState, MonitorEvent};

const UPDATE_APPLIED_NOTICE_WINDOW: Duration = Duration::from_secs(60);

/// Upgrades the connection to a WebSocket streaming [`MonitorEvent`]s as JSON.
/// The complete retained app snapshot is sent on connect and after every
/// retained-state change; one-off events such as `recordingSaved` are forwarded as they occur.
pub async fn handle_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut snapshots = state.snapshot.subscribe();
    let mut events = state.event_tx.subscribe();

    // Announce which build serves this API first, so a stale tab can reload
    // before it starts acting on snapshot or one-off events.
    let version = MonitorEvent::Version { build_id: crate::http::routes::index::BUILD_ID.clone() };
    if send_event(&mut socket, &version).await.is_err() {
        return;
    }

    if send_current_snapshot(&mut socket, &mut snapshots).await.is_err() {
        return;
    }

    // A one-off "plugin updated" notice for any client connecting shortly after
    // this core was loaded via an applied update (not a cold start or rollback),
    // bounded by a grace period so late-connecting clients don't see it stale.
    if state.reloaded_at.is_some_and(|when| when.elapsed() < UPDATE_APPLIED_NOTICE_WINDOW) {
        let settings = state.settings.get();
        let release_url = if settings.last_known_update_version.as_deref() == Some(crate::PLUGIN_VERSION) {
            settings.last_known_update_release_url
        } else {
            None
        };
        let applied = MonitorEvent::UpdateApplied { version: crate::PLUGIN_VERSION.to_owned(), release_url };
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
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            inbound = socket.recv() => {
                match inbound {
                    Some(Ok(_)) => {}
                    _ => break,
                }
            }
        }
    }
}

/// Serializes `event` to JSON and sends it over `socket`. A serialization error
/// is swallowed (the event is skipped); a transport error propagates so the
/// caller can drop the connection.
async fn send_event(socket: &mut WebSocket, event: &MonitorEvent) -> Result<(), axum::Error> {
    if let Ok(text) = serde_json::to_string(event) {
        socket.send(Message::Text(text.into())).await?;
    }
    Ok(())
}

/// Sends the current retained app snapshot, marking the watch value as seen.
async fn send_current_snapshot(
    socket: &mut WebSocket,
    rx: &mut watch::Receiver<crate::http::AppSnapshot>,
) -> Result<(), axum::Error> {
    let state = Box::new(rx.borrow_and_update().clone());
    send_event(socket, &MonitorEvent::Snapshot { state }).await?;
    Ok(())
}
