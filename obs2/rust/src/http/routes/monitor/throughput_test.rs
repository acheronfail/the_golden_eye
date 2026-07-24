use super::*;

fn feed(meter: &mut ThroughputMeter, start: Instant, frames: usize, interval_ms: u64, dropped: u64) -> MonitorFps {
    let mut latest = None;
    for frame in 1..=frames {
        latest = meter.observe(start + Duration::from_millis(frame as u64 * interval_ms), dropped).or(latest);
    }
    latest.expect("meter emitted after warmup")
}

#[test]
fn rolling_rate_ignores_individual_callback_jitter() {
    let start = Instant::now();
    let mut meter = ThroughputMeter::new(start, 30.0);
    let intervals = [34, 32, 34, 33, 33, 34, 32, 34, 33, 33, 34, 32];
    let mut at = start;
    let mut latest = None;
    for interval in intervals {
        at += Duration::from_millis(interval);
        latest = meter.observe(at, 0).or(latest);
    }
    let fps = latest.expect("meter emitted");
    assert!((fps.processed_fps - 30.0).abs() < 0.5);
    assert_eq!(fps.health, MonitorFpsHealth::Healthy);
}

#[test]
fn drops_warn_then_mark_repeated_loss_as_lagging() {
    let start = Instant::now();
    let mut meter = ThroughputMeter::new(start, 30.0);
    let warning = feed(&mut meter, start, 10, 33, 1);
    assert_eq!(warning.health, MonitorFpsHealth::Warning);

    let lagging = feed(&mut meter, start + Duration::from_millis(330), 10, 33, 2);
    assert_eq!(lagging.health, MonitorFpsHealth::Lagging);
}

#[test]
fn health_requires_a_clean_second_to_recover() {
    let start = Instant::now();
    let mut meter = ThroughputMeter::new(start, 30.0);
    let lagging = feed(&mut meter, start, 10, 33, 2);
    assert_eq!(lagging.health, MonitorFpsHealth::Lagging);

    let still_recovering = feed(&mut meter, start + Duration::from_secs(1), 25, 33, 2);
    assert_ne!(still_recovering.health, MonitorFpsHealth::Healthy);
    let recovered = feed(&mut meter, start + Duration::from_secs(2), 35, 33, 2);
    assert_eq!(recovered.health, MonitorFpsHealth::Healthy);
}
