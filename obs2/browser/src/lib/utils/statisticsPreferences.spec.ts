import { describe, expect, it } from 'vitest';
import {
	readStatisticsPreferences,
	STATISTICS_PREFERENCES_STORAGE_KEY,
	writeStatisticsPreferences,
	type StatisticsPreferences
} from './statisticsPreferences';

function memoryStorage(initial: string | null = null) {
	let value = initial;
	return {
		getItem: () => value,
		setItem: (_key: string, next: string) => {
			value = next;
		}
	};
}

const preferences: StatisticsPreferences = {
	version: 1,
	tab: 'outcomes',
	range: { preset: 'custom', customFrom: '2026-07-01', customTo: '2026-07-24' },
	bucket: 'day',
	levelNumber: 7,
	difficultyNumber: 2,
	improvementStatuses: ['complete', 'failed'],
	outcomeStatuses: ['abort', 'kia'],
	outcomeMeasure: 'count',
	levelOrder: 'mission',
	selectedSessionId: 'session-123'
};

describe('statistics preferences', () => {
	it('round trips every statistics control through browser storage', () => {
		const storage = memoryStorage();
		writeStatisticsPreferences(storage, preferences);
		expect(readStatisticsPreferences(storage)).toEqual(preferences);
	});

	it('ignores malformed values while retaining valid preferences', () => {
		const storage = memoryStorage(
			JSON.stringify({
				...preferences,
				tab: 'unknown',
				levelNumber: 99,
				difficultyNumber: -1,
				improvementStatuses: ['complete', 'unknown', 'complete'],
				outcomeStatuses: []
			})
		);

		expect(readStatisticsPreferences(storage)).toEqual(
			expect.objectContaining({
				version: 1,
				bucket: 'day',
				improvementStatuses: ['complete']
			})
		);
		expect(readStatisticsPreferences(storage)).not.toEqual(
			expect.objectContaining({
				tab: expect.anything(),
				levelNumber: expect.anything(),
				difficultyNumber: expect.anything(),
				outcomeStatuses: expect.anything()
			})
		);
	});

	it('rejects unknown versions and invalid JSON', () => {
		expect(readStatisticsPreferences(memoryStorage('{'))).toBeNull();
		expect(
			readStatisticsPreferences(
				memoryStorage(JSON.stringify({ ...preferences, version: 2, key: STATISTICS_PREFERENCES_STORAGE_KEY }))
			)
		).toBeNull();
	});
});
