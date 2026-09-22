use std::sync::OnceLock;
use std::thread;

// Cached count of usable cores. OpenCV is built without TBB/OpenMP, so each
// `match_template` pins one core; independent per-scale/per-template matches are
// spread across spare cores by `par_map`. Queried once; fixed for the process.
fn parallelism() -> usize {
    static N: OnceLock<usize> = OnceLock::new();
    *N.get_or_init(|| thread::available_parallelism().map(|p| p.get()).unwrap_or(1))
}

// Maps `f` over `0..n` in index order, splitting the work into contiguous chunks
// on scoped OS threads so independent template matches run concurrently. Falls
// back to serial for tiny `n` or single-core. Order preserved for replay.
pub(super) fn par_map<T, F>(n: usize, f: F) -> Vec<T>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
{
    let threads = parallelism().min(n);
    if threads <= 1 {
        return (0..n).map(f).collect();
    }
    let chunk = n.div_ceil(threads);
    let f = &f;
    let parts: Vec<Vec<T>> = thread::scope(|s| {
        let handles: Vec<_> = (0..n)
            .step_by(chunk)
            .map(|base| s.spawn(move || (base..(base + chunk).min(n)).map(f).collect::<Vec<T>>()))
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    parts.into_iter().flatten().collect()
}
