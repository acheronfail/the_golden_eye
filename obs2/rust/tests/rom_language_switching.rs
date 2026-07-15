mod support;

use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde_json::Value;
use support::harness::Harness;
use support::test_obs::Frame;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn monitor_detects_rom_language_and_switches_matchers_back_and_forth() {
    let harness = Harness::start(Duration::ZERO).await;
    harness.start_monitor().await.error_for_status().unwrap();
    let (mut ws, _) = connect_async("ws://127.0.0.1:31337/api/v1/monitor/ws").await.unwrap();

    let en_start = harness.frame("test/screenshots-emu/en - start - 01 - Agent.png");
    render_until_match(&harness, &mut ws, &en_start, "initial en start", |m| is_match(m, "Start", 1, 1, 0, Some("en")))
        .await;

    let jp_start = harness.frame("test/screenshots-emu/jp - start - 01 - Agent.png");
    let jp_switch = render_until_match(&harness, &mut ws, &jp_start, "jp language switch", |m| {
        is_match(m, "Unknown", -1, -1, -1, Some("jp"))
    })
    .await;
    assert_eq!(jp_switch["detected_lang"], "jp");

    let jp_match = render_until_match(&harness, &mut ws, &jp_start, "jp start after switch", |m| {
        is_match(m, "Start", 1, 1, 0, Some("jp"))
    })
    .await;
    assert_eq!(jp_match["screen"], "Start");

    let en_switch_start = harness.frame("test/screenshots-av2hdmi/en - start - 3 - 00 Agent - blackbars.png");
    let en_switch = render_until_match(&harness, &mut ws, &en_switch_start, "en language switch", |m| {
        is_match(m, "Unknown", -1, -1, -1, Some("en"))
    })
    .await;
    assert_eq!(en_switch["detected_lang"], "en");

    let en_match_again = render_until_match(&harness, &mut ws, &en_switch_start, "en start after switch back", |m| {
        is_match(m, "Start", 1, 3, 2, Some("en"))
    })
    .await;
    assert_eq!(en_match_again["screen"], "Start");

    harness.stop_monitor().await.error_for_status().unwrap();
}

async fn render_until_match(
    harness: &Harness,
    ws: &mut Ws,
    frame: &Frame,
    label: &str,
    predicate: impl Fn(&Value) -> bool,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_match = None;

    loop {
        harness.obs.render(frame.clone());

        match tokio::time::timeout(Duration::from_millis(120), ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                let value: Value = serde_json::from_str(&text).unwrap();
                if value["type"] == "snapshot" && !value["state"]["match"].is_null() {
                    let matched = value["state"]["match"].clone();
                    last_match = Some(matched.clone());
                    if predicate(&matched) {
                        return matched;
                    }
                }
            }
            Ok(Some(Ok(Message::Binary(bytes)))) => {
                let value: Value = serde_json::from_slice(&bytes).unwrap();
                if value["type"] == "snapshot" && !value["state"]["match"].is_null() {
                    let matched = value["state"]["match"].clone();
                    last_match = Some(matched.clone());
                    if predicate(&matched) {
                        return matched;
                    }
                }
            }
            Ok(Some(Ok(Message::Close(frame)))) => {
                panic!("monitor websocket closed while waiting for {label}: {frame:?}");
            }
            Ok(Some(Ok(_))) | Err(_) => {}
            Ok(Some(Err(err))) => panic!("monitor websocket failed while waiting for {label}: {err}"),
            Ok(None) => panic!("monitor websocket ended while waiting for {label}"),
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for {label}; last match: {}",
            last_match.as_ref().map_or_else(|| "<none>".to_owned(), Value::to_string)
        );
    }
}

fn is_match(
    value: &Value,
    screen: &str,
    mission: i64,
    part: i64,
    difficulty: i64,
    detected_lang: Option<&str>,
) -> bool {
    value["screen"] == screen
        && value["mission"] == mission
        && value["part"] == part
        && value["difficulty"] == difficulty
        && match detected_lang {
            Some(lang) => value["detected_lang"] == lang,
            None => value.get("detected_lang").is_none(),
        }
}
