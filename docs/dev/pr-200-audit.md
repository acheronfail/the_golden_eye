# PR #200 shutdown ownership

The audit of `c9d76bf` found two dependencies that runtime ownership alone did not resolve: replay
completion callbacks were removed before workers finished, and a native folder-picker callback could
outlive its Rust waiter.

## Replay saves and updates

`app/lifecycle.rs` serializes admission with the update decision. Monitor sessions, replay-save
jobs, and folder callbacks hold permits until their work ends. An update closes admission atomically
only when these dependencies are idle; development reload may bypass the monitor permit but still
waits for saves and folder callbacks.

Before C removes frontend callbacks, `ge_runtime_begin_shutdown` closes admission and cancels
replay-coordinator waits. It is also called on OBS's scripting-shutdown event and defensively by
`ge_runtime_stop`. Pending padding is interruptible. Unfinished replays are left on disk, with the
durable run row retained. A replay whose completion was already received can finish trimming, and
runtime teardown joins that work.

Ordinary monitor stop still flushes pending saves. Application exit cancels work that would
otherwise need an OBS event that can no longer arrive; it does not wait 120 seconds or abandon a
thread executing inside the core.

## Native picker workaround

The existing native picker is preserved, with no new Qt dependency. Its HTTP waiter uses an async
oneshot rather than a blocking thread and is dropped at runtime shutdown. The queued callback owns
the activity permit independently of the HTTP request, so closing a browser tab cannot make an open
picker disappear from update eligibility. A callback invoked after admission closes skips opening
the dialog.

The picker library does not expose a cancellation API for an already-open dialog. Before queuing the
first picker, the core retains its own loaded library image until process exit. This protects the
callback, its captured data, and its return path even if OBS drains its UI queue after Rust has
stopped. Failure to retain the image rejects the picker request before any callback is queued. The
user can close an already-open dialog normally; it does not hold a Tokio worker or prevent runtime
teardown.

This is a deliberate resource tradeoff: every core instance that queued a picker stays mapped until
OBS exits, even after a subsequent hot reload. Runtime workers, HTTP ports, and monitoring resources
still stop normally. On Windows the retained temporary DLL may also remain in the OS temp directory
because loaded DLL files cannot be deleted. The loader contract and updater number are unchanged. A
fully cancellable native dialog adapter would avoid this retention, but requires a larger
implementation/dependency change.

## Coverage

- Work admission and update exclusion, including development reload behavior.
- Recording/save timer expiry, stale expiry protection, and runtime timer cancellation.
- Replay cancellation, rejection of new save requests, and interrupted padding/lifecycle waits.
- Queued picker cancellation, open-picker ownership after HTTP cancellation, and dispatch failure.
- Fake-OBS shutdown with a deferred replay completion, raw-file preservation, and rejected update
  application while a save or picker remains outstanding.
- A native shared-library fixture calls into the retained image after releasing the loader handle,
  exercising the actual macOS/Linux/Windows pinning implementation.

The original warning against dropping Tokio from its own worker still applies to actual teardown.
The reload trigger only signals the loader's dedicated OS worker. Replay processing runs in Tokio's
blocking pool; teardown joins it before releasing the loader handle. The retained native UI image is
the explicit exception to physical unmapping.
