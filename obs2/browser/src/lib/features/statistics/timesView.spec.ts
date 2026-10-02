import { describe, expect, it } from 'vitest';
import { completedRun } from '../../../stories/fixtures';
import { bestTime, timesForLevel } from './timesView';

const run = (id: string, metadata: Partial<typeof completedRun.metadata>) => ({
	...completedRun,
	runId: id,
	path: '',
	metadata: { ...completedRun.metadata, ...metadata }
});

describe('best times and history', () => {
	it('includes old metadata-only runs and ties, excluding other cohorts and unsuccessful or missing times', () => {
		const runs = [
			run('old-best', { timestamp: '2020-01-01T00:00:00Z', timeSeconds: 50 }),
			run('tie', { timestamp: '2026-01-01T00:00:00Z', timeSeconds: 50 }),
			run('recent', { timestamp: '2026-02-01T00:00:00Z', timeSeconds: 60 }),
			run('failed', { status: 'failed', timeSeconds: 20 }),
			run('other-level', { levelNumber: 1 }),
			run('other-difficulty', { difficulty: 'Agent' }),
			run('no-time', { timeSeconds: undefined, time: undefined })
		];
		const history = timesForLevel(runs, 2, '00 Agent');
		expect(history.map((entry) => entry.runId)).toEqual(['recent', 'tie', 'old-best']);
		expect(bestTime(history)).toBe(50);
		expect(bestTime(timesForLevel(runs, 3, 'Agent'))).toBeNull();
	});

	it('supports legacy level names and formatted times', () => {
		const history = timesForLevel(
			[run('legacy', { levelNumber: undefined, timeSeconds: undefined, time: '1:02' })],
			2,
			'00 Agent'
		);
		expect(bestTime(history)).toBe(62);
	});
});
