import { afterEach, expect, it, vi } from 'vitest';
import type { RecordingSavePending, RunClip, RunsResponse } from '$lib/api';
import { PersonalBestCelebrations } from './personalBestCelebrations';

const pb: RunClip = {
	retentionState: 'kept',
	retentionReason: 'personalBest',
	runId: 'pb-1',
	path: '',
	fileName: '',
	directory: '',
	sizeBytes: 0,
	metadata: {
		timestamp: '2026-10-08T00:00:00Z',
		level: 'Dam',
		levelNumber: 1,
		difficulty: 'Agent',
		status: 'complete',
		timeSeconds: 52,
		wasPersonalBest: true,
		sourceName: 'N64',
		gameLanguage: 'en',
		comment: '',
		pluginVersion: 'test'
	}
};
const response = (run = pb): RunsResponse => ({ clips: [], directories: [], requestedRun: run });
const fixture = () => {
	const getRuns = vi.fn().mockResolvedValue(response());
	const celebrate = vi.fn();
	const getBestTimes = vi.fn().mockResolvedValue([{ ...pb, metadata: { ...pb.metadata, timeSeconds: 55 } }]);
	let source: string | null = 'N64';
	return {
		getRuns,
		getBestTimes,
		celebrate,
		changeSource: (next: string | null) => {
			source = next;
		},
		controller: new PersonalBestCelebrations({ getRuns, getBestTimes }, () => source, celebrate)
	};
};

it('celebrates a confirmed PB once across duplicate and concurrent catalog events', async () => {
	const f = fixture();
	await Promise.all([
		f.controller.handle({ runId: 'pb-1', saveId: 1 }),
		f.controller.handle({ runId: 'pb-1', saveId: 1 })
	]);
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	expect(f.celebrate).toHaveBeenCalledTimes(1);
	expect(f.getRuns).toHaveBeenCalledTimes(1);
});

it('celebrates a later PB independently', async () => {
	const f = fixture();
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	f.getRuns.mockResolvedValue(response({ ...pb, runId: 'pb-2' }));
	await f.controller.handle({ runId: 'pb-2', saveId: 2 });
	expect(f.celebrate).toHaveBeenCalledTimes(2);
});

it.each([{ wasPersonalBest: false }, { status: 'failed' }, { sourceName: 'Other' }])(
	'does not celebrate %j',
	async (metadata) => {
		const f = fixture();
		f.getRuns.mockResolvedValue(response({ ...pb, metadata: { ...pb.metadata, ...metadata } }));
		await f.controller.handle({ runId: 'pb-1', saveId: 1 });
		expect(f.celebrate).not.toHaveBeenCalled();
	}
);

it('ignores historical edits, imports, and events outside monitoring', async () => {
	const f = fixture();
	await f.controller.handle({ runId: 'pb-1' });
	await f.controller.handle({ saveId: 1 });
	f.changeSource(null);
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	expect(f.getRuns).not.toHaveBeenCalled();
	expect(f.celebrate).not.toHaveBeenCalled();
});

it.each([null, 'Other'])('ignores a response after the active source changes to %s', async (source) => {
	const f = fixture();
	let resolve!: (value: RunsResponse) => void;
	f.getRuns.mockReturnValue(
		new Promise<RunsResponse>((done) => {
			resolve = done;
		})
	);
	const pending = f.controller.handle({ runId: 'pb-1', saveId: 1 });
	f.changeSource(source);
	resolve(response());
	await pending;
	expect(f.celebrate).not.toHaveBeenCalled();
});

it('allows the later clip completion event to retry a failed catalog request', async () => {
	const f = fixture();
	f.getRuns.mockRejectedValueOnce(new Error('unavailable'));
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	expect(f.celebrate).toHaveBeenCalledTimes(1);
});

const pending: RecordingSavePending = {
	saveId: 1,
	saveInSecs: 10,
	estimatedDurationSecs: 60,
	failed: false,
	status: 'complete',
	level: 'Dam',
	levelNumber: 1,
	difficulty: 'Agent',
	timeSecs: 52
};

afterEach(() => vi.useRealTimers());

it('celebrates a stable pending PB before saving, without repeating on finalization', async () => {
	vi.useFakeTimers();
	const f = fixture();
	f.controller.handlePending(pending);
	await vi.advanceTimersByTimeAsync(999);
	expect(f.celebrate).not.toHaveBeenCalled();
	await vi.advanceTimersByTimeAsync(1);
	expect(f.celebrate).toHaveBeenCalledTimes(1);
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	expect(f.celebrate).toHaveBeenCalledTimes(1);
	expect(f.getRuns).not.toHaveBeenCalled();
});

it.each([55, 56])('does not celebrate a pending time of %s against a 55-second PB', async (timeSecs) => {
	vi.useFakeTimers();
	const f = fixture();
	f.controller.handlePending({ ...pending, timeSecs });
	await vi.advanceTimersByTimeAsync(1000);
	expect(f.celebrate).not.toHaveBeenCalled();
});

it('lets a corrected stats reading cancel a provisional PB', async () => {
	vi.useFakeTimers();
	const f = fixture();
	f.controller.handlePending(pending);
	await vi.advanceTimersByTimeAsync(500);
	f.controller.handlePending({ ...pending, timeSecs: 56 });
	await vi.advanceTimersByTimeAsync(1000);
	expect(f.celebrate).not.toHaveBeenCalled();
});

it('celebrates the first completed time for a level and difficulty', async () => {
	vi.useFakeTimers();
	const f = fixture();
	f.getBestTimes.mockResolvedValue([]);
	f.controller.handlePending(pending);
	await vi.advanceTimersByTimeAsync(1000);
	expect(f.celebrate).toHaveBeenCalledTimes(1);
});

it.each([
	{ status: 'failed' },
	{ failed: true },
	{ timeSecs: undefined },
	{ levelNumber: undefined },
	{ difficulty: undefined }
])('ignores ineligible pending results %j', async (overrides) => {
	vi.useFakeTimers();
	const f = fixture();
	f.controller.handlePending({ ...pending, ...overrides });
	await vi.advanceTimersByTimeAsync(1000);
	expect(f.getBestTimes).not.toHaveBeenCalled();
	expect(f.celebrate).not.toHaveBeenCalled();
});

it('falls back to finalization when the early PB lookup fails', async () => {
	vi.useFakeTimers();
	const f = fixture();
	f.getBestTimes.mockRejectedValue(new Error('unavailable'));
	f.controller.handlePending(pending);
	await vi.advanceTimersByTimeAsync(1000);
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	expect(f.celebrate).toHaveBeenCalledTimes(1);
});

it('discards an in-flight pending lookup when the result is finalized', async () => {
	vi.useFakeTimers();
	const f = fixture();
	let resolve!: (value: RunClip[]) => void;
	f.getBestTimes.mockReturnValue(
		new Promise<RunClip[]>((done) => {
			resolve = done;
		})
	);
	f.controller.handlePending(pending);
	await vi.advanceTimersByTimeAsync(1000);
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	resolve([]);
	await Promise.resolve();
	expect(f.celebrate).toHaveBeenCalledTimes(1);
});

it('cancels pending celebrations on reset and permits save IDs in a new session', async () => {
	vi.useFakeTimers();
	const f = fixture();
	f.controller.handlePending(pending);
	f.controller.reset();
	await vi.advanceTimersByTimeAsync(1000);
	expect(f.celebrate).not.toHaveBeenCalled();
	f.controller.handlePending(pending);
	await vi.advanceTimersByTimeAsync(1000);
	f.controller.reset();
	f.controller.handlePending(pending);
	await vi.advanceTimersByTimeAsync(1000);
	expect(f.celebrate).toHaveBeenCalledTimes(2);
});

it('does not postpone celebration when only the target or game best time changes', async () => {
	vi.useFakeTimers();
	const f = fixture();
	f.controller.handlePending(pending);
	await vi.advanceTimersByTimeAsync(500);
	f.controller.handlePending({ ...pending, targetTimeSecs: 60, bestTimeSecs: 52 });
	await vi.advanceTimersByTimeAsync(500);
	expect(f.celebrate).toHaveBeenCalledTimes(1);
});

it.each(['correction', 'reset', 'source'])('discards an in-flight early lookup after %s', async (change) => {
	vi.useFakeTimers();
	const f = fixture();
	let resolve!: (value: RunClip[]) => void;
	f.getBestTimes.mockReturnValue(
		new Promise<RunClip[]>((done) => {
			resolve = done;
		})
	);
	f.controller.handlePending(pending);
	await vi.advanceTimersByTimeAsync(1000);
	if (change === 'correction') f.controller.handlePending({ ...pending, timeSecs: 56 });
	if (change === 'reset') f.controller.reset();
	if (change === 'source') f.changeSource('Other');
	resolve([]);
	await Promise.resolve();
	expect(f.celebrate).not.toHaveBeenCalled();
	f.controller.reset();
});

it('cancels the early timer when finalization arrives first', async () => {
	vi.useFakeTimers();
	const f = fixture();
	f.controller.handlePending(pending);
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	await vi.advanceTimersByTimeAsync(1000);
	expect(f.celebrate).toHaveBeenCalledTimes(1);
	expect(f.getBestTimes).not.toHaveBeenCalled();
});
