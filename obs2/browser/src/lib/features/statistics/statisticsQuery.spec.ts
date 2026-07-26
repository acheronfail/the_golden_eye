import { describe, expect, it } from 'vitest';
import { defaultDateRange } from '$lib/features/statistics/statisticsRange';
import { statisticsRouteHref, statisticsRouteState } from './statisticsQuery';

describe('statistics query state', () => {
	it('parses valid values and rejects invalid cohorts', () => {
		const state = statisticsRouteState(
			new URL('http://localhost/statistics?tab=outcomes&range=7d&bucket=day&level=21&difficulty=2'),
			defaultDateRange()
		);

		expect(state).toMatchObject({
			tab: 'outcomes',
			range: { preset: '7d' },
			bucket: 'day',
			levelNumber: 1,
			difficultyNumber: 2
		});
	});

	it('omits defaults and stale custom dates when serializing', () => {
		const href = statisticsRouteHref(new URL('http://localhost/statistics?fromDate=old&toDate=old'), {
			tab: 'overview',
			range: { ...defaultDateRange(), preset: '30d' },
			bucket: 'week',
			levelNumber: 1,
			difficultyNumber: 0
		});

		expect(href).toBe('/statistics');
	});

	it('serializes custom dates with the selected cohort', () => {
		const href = statisticsRouteHref(new URL('http://localhost/statistics'), {
			tab: 'improvement',
			range: { preset: 'custom', customFrom: '2026-01-01', customTo: '2026-01-31' },
			bucket: 'month',
			levelNumber: 4,
			difficultyNumber: 2
		});

		expect(href).toContain('tab=improvement');
		expect(href).toContain('fromDate=2026-01-01');
		expect(href).toContain('level=4');
	});
});
