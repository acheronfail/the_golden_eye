import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RecordingSavePending, ReplaySaveStatus, RunClip } from '$lib/api';
import { RecentRunsStore } from './recentRuns.svelte';

const mocks = vi.hoisted(() => ({
	getRecentRuns: vi.fn(),
	keepRun: vi.fn()
}));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		backend: { ...actual.backend, getRecentRuns: mocks.getRecentRuns, keepRun: mocks.keepRun }
	};
});

const pending = (overrides: Partial<RecordingSavePending> = {}): RecordingSavePending => ({
	trackingId: 41,
	saveId: 7,
	saveInSecs: 5,
	estimatedDurationSecs: 75,
	failed: false,
	status: 'complete',
	level: 'Facility',
	levelNumber: 2,
	difficulty: '00 Agent',
	timeSecs: 58,
	...overrides
});

const saving = (overrides: Partial<ReplaySaveStatus> = {}): ReplaySaveStatus => ({
	trackingId: 41,
	saveId: 7,
	stage: 'savingReplay',
	level: 'Facility',
	difficulty: '00 Agent',
	runStatus: 'complete',
	estimatedDurationSecs: 75,
	...overrides
});

const finalized: RunClip = {
	runId: 'finalized-run',
	path: '',
	fileName: '',
	directory: '',
	sizeBytes: 0,
	metadata: {
		runId: 'finalized-run',
		timestamp: '2026-07-23T10:00:00Z',
		time: '00:58',
		timeSeconds: 58,
		level: 'Facility',
		levelNumber: 2,
		difficulty: '00 Agent',
		status: 'complete',
		gameLanguage: 'en',
		sourceName: 'N64 Capture',
		comment: '',
		pluginVersion: 'test'
	},
	retentionState: 'kept',
	retentionReason: 'personalBest'
};

describe('recent runs store', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	it('shows and refines one provisional row for a pending save', () => {
		const store = new RecentRunsStore();
		store.applySavePending(pending({ timeSecs: 374 }));
		const timestamp = store.items[0].metadata.timestamp;

		expect(store.items).toHaveLength(1);
		expect(store.items[0].runId).toBe('pending-41');
		expect(store.items[0].metadata.status).toBe('pending');
		expect(store.items[0].metadata.time).toBe('06:14');

		store.applySavePending(pending({ timeSecs: 14 }));
		expect(store.items).toHaveLength(1);
		expect(store.items[0].metadata.timestamp).toBe(timestamp);
		expect(store.items[0].metadata.time).toBe('00:14');
	});

	it('replaces the provisional row with the finalized catalog run', async () => {
		const store = new RecentRunsStore();
		store.applySavePending(pending());
		mocks.getRecentRuns.mockResolvedValue([finalized]);
		store.applyReplaySaves([saving({ runId: finalized.runId })]);

		await store.refresh();

		expect(store.items).toEqual([{ ...finalized, saving: true }]);
	});

	it('associates an active save by run ID after reload and displays the persisted failure when refreshed', async () => {
		const store = new RecentRunsStore();
		const old = { ...finalized, runId: 'old-run' };
		mocks.getRecentRuns.mockResolvedValue([finalized, old]);
		store.applyReplaySaves([saving({ runId: finalized.runId, stage: 'trimming' })]);
		await store.refresh();
		expect(store.items).toEqual([{ ...finalized, saving: true }, old]);

		const failed = { ...finalized, clipSaveError: 'OBS replay buffer save timed out' };
		mocks.getRecentRuns.mockResolvedValue([failed, old]);
		store.applyReplaySaves([saving({ runId: finalized.runId, stage: 'failed' })]);
		await store.refresh();
		expect(store.items).toEqual([failed, old]);
	});

	it('ignores a late pending event after a save ends and accepts a new session reusing its save ID', () => {
		const store = new RecentRunsStore();
		store.applySavePending(pending());
		store.applyReplaySaves([saving({ stage: 'failed' })]);
		expect(store.items).toEqual([]);

		store.applySavePending(pending());
		store.applySavePending(pending({ trackingId: 42 }));
		expect(store.items.map((run) => run.runId)).toEqual(['pending-42']);
	});

	it('always requests and displays the maximum recent-run history', async () => {
		const runs = Array.from({ length: 20 }, (_, index) => ({
			...finalized,
			runId: `run-${index}`,
			metadata: { ...finalized.metadata, runId: `run-${index}` }
		}));
		mocks.getRecentRuns.mockResolvedValue(runs);
		const store = new RecentRunsStore();

		await store.refresh();

		expect(mocks.getRecentRuns).toHaveBeenCalledWith(20);
		expect(store.items).toHaveLength(20);
	});
});
