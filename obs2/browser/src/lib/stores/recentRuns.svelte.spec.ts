import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RecordingSavePending, RunClip } from '$lib/api';
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
		romLanguage: 'en',
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
		expect(store.items[0].runId).toBeUndefined();
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

		await store.refresh(7);

		expect(store.items).toEqual([finalized]);
	});
});
