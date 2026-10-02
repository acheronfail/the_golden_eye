use std::time::Duration;

use serde_json::{Value, json};

use crate::support::harness::{API, Harness, recording_settings};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run explicitly with `just test-integration`"]
async fn times_overview_and_filtered_history_are_bounded() {
    let harness = Harness::start_with_settings_from_temp(Duration::ZERO, |temp| {
        recording_settings(&temp.join("completed"), &temp.join("failed"))
    })
    .await;
    let mut ids = Vec::new();
    for (date, level, difficulty, time) in [
        ("2026-01-01", "Dam", "Agent", "0:50"),
        ("2026-01-02", "Dam", "Agent", "0:55"),
        ("2026-01-03", "Dam", "Agent", "0:50"),
        ("2026-01-04", "Dam", "Secret Agent", "1:00"),
        ("2026-01-05", "Facility", "Agent", "0:45"),
        ("2026-01-06", "Dam", "007", "0:01"),
    ] {
        let run: Value = harness
            .client
            .post(format!("{API}/api/v1/runs/manual"))
            .json(
                &json!({ "date": date, "level": level, "difficulty": difficulty, "time": time, "gameLanguage": "en" }),
            )
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        ids.push(run["runId"].as_str().unwrap().to_owned());
    }
    let bests: Vec<Value> = harness
        .client
        .get(format!("{API}/api/v1/runs/best-times"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(bests.len(), 3);
    assert_eq!(bests[0]["runId"], ids[2]);
    assert_eq!(bests[0]["path"], "");

    let mut cursor: Option<String> = None;
    let mut history = Vec::new();
    loop {
        let mut params = vec![
            ("level", "Dam"),
            ("difficulty", "Agent"),
            ("status", "complete"),
            ("minTimeSeconds", "0"),
            ("sort", "newest"),
            ("limit", "2"),
        ];
        if let Some(ref value) = cursor {
            params.push(("cursor", value));
        }
        let mut url = reqwest::Url::parse(&format!("{API}/api/v1/runs")).unwrap();
        url.query_pairs_mut().extend_pairs(params);
        let page: Value =
            harness.client.get(url).send().await.unwrap().error_for_status().unwrap().json().await.unwrap();
        let clips = page["clips"].as_array().unwrap();
        assert!(clips.len() <= 2);
        history.extend(clips.iter().map(|run| run["runId"].as_str().unwrap().to_owned()));
        cursor = page["nextCursor"].as_str().map(str::to_owned);
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(history, [ids[2].clone(), ids[1].clone(), ids[0].clone()]);
}
