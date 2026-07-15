use super::*;

fn owned_frame(tag: u8, width: u32) -> Frame {
    Frame {
        buf: FrameBuf::Owned(vec![tag]),
        width,
        height: 1,
        captured_at: None,
        capture_ms: None,
        dropped_frames_total: 0,
    }
}

#[test]
fn mailbox_capacity_one_keeps_only_the_latest_frame() {
    let mailbox = FrameMailbox::new(1);
    // Two pushes with no intervening recv: at capacity 1 the newer frame
    // evicts (and frees) the older one, so only the latest is delivered.
    mailbox.push(owned_frame(1, 10));
    mailbox.push(owned_frame(2, 20));
    let frame = mailbox.recv().expect("a frame is buffered");
    assert_eq!(frame.width, 20, "newest frame wins");
    assert_eq!(frame.buf.as_slice(), &[2]);
}

#[test]
fn mailbox_buffers_up_to_capacity_then_drops_oldest() {
    let mailbox = FrameMailbox::new(2);
    // Within capacity, frames are retained and delivered oldest-first.
    mailbox.push(owned_frame(1, 10));
    mailbox.push(owned_frame(2, 20));
    // A third push overflows: the oldest (frame 1) is dropped.
    mailbox.push(owned_frame(3, 30));
    assert_eq!(mailbox.recv().expect("first").width, 20, "oldest survivor first");
    assert_eq!(mailbox.recv().expect("second").width, 30, "then the newest");
}

#[test]
fn mailbox_recv_returns_none_once_closed_and_drained() {
    let mailbox = FrameMailbox::new(1);
    // A frame still buffered at close is drained before recv reports closed.
    mailbox.push(owned_frame(7, 30));
    mailbox.close();
    assert_eq!(mailbox.recv().expect("drains the buffered frame").width, 30);
    assert!(mailbox.recv().is_none(), "closed and drained -> None");
    // A push after close is dropped, not stored.
    mailbox.push(owned_frame(9, 40));
    assert!(mailbox.recv().is_none(), "push after close is a no-op");
}
