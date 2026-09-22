use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use super::run_update_checks;

#[tokio::test(start_paused = true)]
async fn periodic_checks_wait_fifteen_minutes_after_failure_and_success() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let checked = attempts.clone();
    let task = tokio::spawn(async move {
        run_update_checks(|| {
            let checked = checked.clone();
            async move {
                if checked.fetch_add(1, Ordering::SeqCst) == 0 {
                    anyhow::bail!("simulated release check failure");
                }
                Ok(None)
            }
        })
        .await;
    });

    tokio::task::yield_now().await;
    assert_eq!(attempts.load(Ordering::SeqCst), 1, "check immediately at startup");

    for completed in 1..=2 {
        tokio::time::advance(Duration::from_secs(14 * 60 + 59)).await;
        tokio::task::yield_now().await;
        assert_eq!(attempts.load(Ordering::SeqCst), completed, "do not check before fifteen minutes");

        tokio::time::advance(Duration::from_secs(1)).await;
        tokio::task::yield_now().await;
        assert_eq!(attempts.load(Ordering::SeqCst), completed + 1, "check again after fifteen minutes");
    }

    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
}
