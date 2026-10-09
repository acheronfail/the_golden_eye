use std::path::PathBuf;

use serde::Deserialize;

use crate::CvMatcher;

const TEMPLATES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../cv_templates");

#[derive(Deserialize)]
struct Capture {
    width: u32,
    height: u32,
    stationary: Vec<Phase>,
    transitions: Vec<Phase>,
}

#[derive(Deserialize)]
struct Phase {
    name: String,
    screen: String,
    mission: i32,
    part: i32,
    difficulty: i32,
    raw_times: Vec<i32>,
    frames: Vec<String>,
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../frame_tests")
}

fn capture() -> Capture {
    serde_json::from_slice(&std::fs::read(fixture_root().join("sequences/archives-gpu/manifest.json")).unwrap())
        .unwrap()
}

fn matcher(prime: Option<&str>) -> CvMatcher {
    let matcher = CvMatcher::new("en", TEMPLATES).unwrap();
    if let Some(prime) = prime {
        let bytes = std::fs::read(fixture_root().join("screenshots-retrogem").join(prime)).unwrap();
        let result = matcher.match_level_from_encoded_image(&bytes).unwrap().0;
        assert_eq!(result.screen.as_str(), "start", "priming frame {prime}");
    }
    matcher
}

fn assert_phase(matcher: &CvMatcher, capture: &Capture, phase: &Phase, prime: Option<&str>) {
    for frame in &phase.frames {
        let bytes = std::fs::read(fixture_root().join("sequences/archives-gpu").join(frame)).unwrap();
        for iteration in 0..3 {
            let (result, width, height) = matcher.match_level_from_encoded_image(&bytes).unwrap();
            assert_eq!((width, height), (capture.width, capture.height), "{frame}");
            assert_eq!(
                (result.screen.as_str(), result.mission, result.part, result.difficulty),
                (phase.screen.as_str(), phase.mission, phase.part, phase.difficulty),
                "phase={}, frame={frame}, prime={prime:?}, iteration={iteration}",
                phase.name,
            );
            assert_eq!(result.raw_times, phase.raw_times, "{frame}, prime={prime:?}");
            if phase.screen == "unknown" {
                assert!(result.times.is_none(), "{frame}");
            } else {
                assert_eq!(result.detected_lang.as_deref(), Some("en"), "{frame}");
            }
            if phase.screen == "stats" {
                let times = result.times.expect("statistics must retain classified times");
                assert_eq!(times.time, 6, "{frame}");
                assert_eq!(times.best_time, Some(600), "{frame}");
            }
        }
    }
}

const PRIMES: [Option<&str>; 3] =
    [None, Some("en - start - 13 - Secret Agent - depot-gpu-capture.png"), Some("en - start - 01 - 00 Agent.png")];

#[test]
fn archives_gpu_stationary_screens_match_every_frame_with_cold_and_warm_caches() {
    let capture = capture();
    for prime in PRIMES {
        for phase in &capture.stationary {
            let matcher = matcher(prime);
            assert_phase(&matcher, &capture, phase, prime);
        }
    }
}

#[test]
fn archives_gpu_transitions_recover_on_the_first_visible_frame() {
    let capture = capture();
    for prime in PRIMES {
        let matcher = matcher(prime);
        for _ in 0..2 {
            for phase in &capture.transitions {
                assert_phase(&matcher, &capture, phase, prime);
            }
            let statistics = capture.stationary.iter().find(|phase| phase.screen == "stats").unwrap();
            assert_phase(&matcher, &capture, statistics, prime);
        }
    }
}
