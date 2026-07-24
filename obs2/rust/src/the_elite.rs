use std::error::Error;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use chrono::{NaiveDate, TimeZone, Utc};
use scraper::{Html, Selector};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

const BASE_URL: &str = "https://rankings.the-elite.net";
const MAX_PROOF_REQUESTS: usize = 8;

#[derive(Debug)]
pub(crate) struct UserNotFound {
    username: String,
}

impl UserNotFound {
    pub(crate) fn new(username: impl Into<String>) -> Self {
        Self { username: username.into() }
    }
}

impl fmt::Display for UserNotFound {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "The Elite user ~{} was not found", self.username)
    }
}

impl Error for UserNotFound {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EliteRun {
    pub time_id: String,
    pub timestamp: String,
    pub level: String,
    pub difficulty: String,
    pub time: String,
    pub time_seconds: i32,
    pub system: String,
    pub current_personal_best: bool,
    pub proof_available: bool,
    pub video_id: Option<String>,
}

pub async fn fetch_history(username: &str) -> anyhow::Result<Vec<EliteRun>> {
    let username = validate_username(username)?;
    let client = reqwest::Client::builder()
        .user_agent(format!("The Golden Eye/{}", env!("GE_PLUGIN_VERSION")))
        .timeout(Duration::from_secs(20))
        .cookie_store(true)
        .build()
        .context("building The Elite client")?;
    let history_url = format!("{BASE_URL}/~{username}/goldeneye/history");
    let response = client.get(&history_url).send().await.context("downloading The Elite history")?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(UserNotFound::new(username).into());
    }
    anyhow::ensure!(response.status().is_success(), "The Elite returned {}", response.status());
    let initial_html = response.text().await.context("reading The Elite history")?;
    let sid = parse_filter_sid(&initial_html).context("The Elite history did not include its filter token")?;
    let response = client
        .post(&history_url)
        .form(&[
            ("sid", sid.as_str()),
            ("date_start", ""),
            ("date_end", ""),
            ("stage_id", ""),
            ("difficulty-0", "0"),
            ("difficulty-1", "1"),
            ("difficulty-2", "2"),
            ("system-0", "NTSC"),
            ("system-1", "NTSC-J"),
            ("system-2", "PAL"),
            ("system-3", "Unknown"),
            ("current_pr", "0"),
        ])
        .send()
        .await
        .context("applying The Elite all-times filter")?;
    anyhow::ensure!(response.status().is_success(), "The Elite filter returned {}", response.status());
    let html = response.text().await.context("reading filtered The Elite history")?;
    let runs = parse_history(&html)?;
    anyhow::ensure!(!runs.is_empty(), "The Elite history did not contain any GoldenEye times");

    let semaphore = Arc::new(Semaphore::new(MAX_PROOF_REQUESTS));
    let mut tasks = JoinSet::new();
    for (index, run) in runs.iter().enumerate().filter(|(_, run)| run.proof_available) {
        let client = client.clone();
        let semaphore = semaphore.clone();
        let time_id = run.time_id.clone();
        tasks.spawn(async move {
            let _permit = semaphore.acquire_owned().await.context("acquiring proof request slot")?;
            let Ok(response) = client.get(format!("{BASE_URL}/video/{time_id}")).send().await else {
                return Ok::<_, anyhow::Error>((index, None));
            };
            if !response.status().is_success() {
                return Ok::<_, anyhow::Error>((index, None));
            }
            let Ok(html) = response.text().await else {
                return Ok((index, None));
            };
            Ok((index, parse_youtube_video_id(&html)))
        });
    }

    let mut runs = runs;
    while let Some(result) = tasks.join_next().await {
        let (index, video_id) = result.context("joining The Elite proof request")??;
        runs[index].video_id = video_id;
    }
    Ok(runs)
}

fn validate_username(value: &str) -> anyhow::Result<&str> {
    let value = value.trim().trim_start_matches('~');
    anyhow::ensure!(
        !value.is_empty()
            && value.len() <= 64
            && value.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-')),
        "enter a valid The Elite username"
    );
    Ok(value)
}

fn parse_history(html: &str) -> anyhow::Result<Vec<EliteRun>> {
    let document = Html::parse_document(html);
    let row_selector = Selector::parse("table tr").expect("valid row selector");
    let cell_selector = Selector::parse("td").expect("valid cell selector");
    let time_selector = Selector::parse("a.time").expect("valid time selector");
    let proof_selector = Selector::parse("a.video-link").expect("valid proof selector");
    let mut runs = Vec::new();

    for row in document.select(&row_selector) {
        let cells = row.select(&cell_selector).collect::<Vec<_>>();
        if cells.len() < 8 {
            continue;
        }
        let time_link = cells[3].select(&time_selector).next().context("history row has no time link")?;
        let href = time_link.value().attr("href").context("history time has no URL")?;
        let time_id = href
            .rsplit('/')
            .next()
            .filter(|value| value.chars().all(|character| character.is_ascii_digit()))
            .context("history time has an invalid ID")?;
        let date = text(&cells[0]);
        let achieved =
            NaiveDate::parse_from_str(&date, "%e %b %Y").with_context(|| format!("parsing The Elite date {date:?}"))?;
        let timestamp = Utc.from_utc_datetime(&achieved.and_hms_opt(12, 0, 0).unwrap()).to_rfc3339();
        let raw_time = text(&cells[3]);
        let time_seconds = parse_time(&raw_time)?;
        let time = format!("{:02}:{:02}", time_seconds / 60, time_seconds % 60);

        runs.push(EliteRun {
            time_id: time_id.to_owned(),
            timestamp,
            level: text(&cells[1]),
            difficulty: text(&cells[2]),
            time_seconds,
            time,
            system: text(&cells[5]),
            current_personal_best: text(&cells[6]).eq_ignore_ascii_case("yes"),
            proof_available: cells[7].select(&proof_selector).next().is_some(),
            video_id: None,
        });
    }
    Ok(runs)
}

fn parse_filter_sid(html: &str) -> Option<String> {
    let document = Html::parse_document(html);
    let selector = Selector::parse("form input[name=sid]").expect("valid filter token selector");
    document
        .select(&selector)
        .find_map(|input| input.value().attr("value"))
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn text(element: &scraper::ElementRef<'_>) -> String {
    element.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_time(value: &str) -> anyhow::Result<i32> {
    let (minutes, seconds) = value.split_once(':').context("The Elite time is not mm:ss")?;
    let minutes = minutes.parse::<i32>().context("parsing The Elite time minutes")?;
    let seconds = seconds.parse::<i32>().context("parsing The Elite time seconds")?;
    if !(0..60).contains(&seconds) || minutes < 0 {
        return Err(anyhow!("The Elite time is outside the supported range"));
    }
    Ok(minutes * 60 + seconds)
}

fn parse_youtube_video_id(html: &str) -> Option<String> {
    ["youtube.com/embed/", "youtube.com/watch?v=", "youtu.be/"].into_iter().find_map(|marker| {
        let start = html.find(marker)? + marker.len();
        let id = html[start..]
            .chars()
            .take_while(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
            .collect::<String>();
        (!id.is_empty()).then_some(id)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HISTORY: &str = r#"
        <table><tr><th>Date Achieved</th></tr><tr>
        <td>24 Jul 2026</td><td>Frigate</td><td>Agent</td>
        <td><a href="/~acheronfail/time/309706" class="time">0:33</a></td>
        <td>0</td><td>NTSC-J</td><td>Yes</td>
        <td><a href="javascript:Site.openOverlay('/video/309706')" class="video-link">Video</a></td>
        </tr></table>
    "#;

    #[test]
    fn parses_history_rows_and_stable_time_ids() {
        let runs = parse_history(HISTORY).unwrap();
        assert_eq!(
            runs,
            vec![EliteRun {
                time_id: "309706".to_owned(),
                timestamp: "2026-07-24T12:00:00+00:00".to_owned(),
                level: "Frigate".to_owned(),
                difficulty: "Agent".to_owned(),
                time: "00:33".to_owned(),
                time_seconds: 33,
                system: "NTSC-J".to_owned(),
                current_personal_best: true,
                proof_available: true,
                video_id: None,
            }]
        );
    }

    #[test]
    fn keeps_history_rows_without_video_proof() {
        let history = r#"
            <table><tr>
            <td>1 Jan 2020</td><td>Dam</td><td>Secret Agent</td>
            <td><a href="/~runner/time/123" class="time">1:23</a></td>
            <td>0</td><td>PAL</td><td>No</td><td></td>
            </tr></table>
        "#;

        let runs = parse_history(history).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].time_id, "123");
        assert!(!runs[0].proof_available);
        assert_eq!(runs[0].video_id, None);
    }

    #[test]
    fn parses_supported_youtube_proof_urls() {
        assert_eq!(
            parse_youtube_video_id(r#"<iframe src="https://www.youtube.com/embed/bgddOpQBKk4?rel=0"></iframe>"#),
            Some("bgddOpQBKk4".to_owned())
        );
        assert_eq!(
            parse_youtube_video_id(r#"<a href="https://www.youtube.com/watch?v=bgddOpQBKk4">Watch</a>"#),
            Some("bgddOpQBKk4".to_owned())
        );
    }

    #[test]
    fn parses_filter_token_for_explicit_all_times_request() {
        assert_eq!(
            parse_filter_sid(r#"<form><input type="hidden" name="sid" value="session-token"></form>"#),
            Some("session-token".to_owned())
        );
    }

    #[test]
    fn validates_usernames() {
        assert_eq!(validate_username("~acheronfail").unwrap(), "acheronfail");
        assert!(validate_username("../profile").is_err());
        assert!(validate_username("first last").is_err());
    }
}
