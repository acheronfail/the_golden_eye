import { render, screen, waitFor } from '@testing-library/svelte';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { statisticsFixture } from '../../stories/statisticsFixtures';
import { STATISTICS_PREFERENCES_STORAGE_KEY } from '$lib/utils/statisticsPreferences';
import StatisticsPage from './+page.svelte';

const mocks = vi.hoisted(() => ({
	getStatistics: vi.fn(),
	getStatisticsSessions: vi.fn(),
	getStatisticsSession: vi.fn(),
	goto: vi.fn(),
	pageUrl: new URL('http://localhost/statistics')
}));

vi.mock('$app/state', () => ({
	page: {
		get url() {
			return mocks.pageUrl;
		}
	}
}));

vi.mock('$app/navigation', () => ({ goto: mocks.goto }));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		backend: {
			...actual.backend,
			getStatistics: mocks.getStatistics,
			getStatisticsSessions: mocks.getStatisticsSessions,
			getStatisticsSession: mocks.getStatisticsSession
		}
	};
});

vi.stubGlobal(
	'ResizeObserver',
	class {
		observe() {}
		disconnect() {}
	}
);

beforeEach(() => {
	localStorage.clear();
	mocks.pageUrl = new URL('http://localhost/statistics');
	mocks.getStatistics.mockResolvedValue(statisticsFixture);
	mocks.getStatisticsSessions.mockResolvedValue([]);
});

describe('/statistics', () => {
	it('restores view and filters from browser storage', async () => {
		localStorage.setItem(
			STATISTICS_PREFERENCES_STORAGE_KEY,
			JSON.stringify({
				version: 1,
				tab: 'outcomes',
				range: { preset: '12m', customFrom: '2026-01-01', customTo: '2026-07-24' },
				bucket: 'month',
				levelNumber: 7,
				difficultyNumber: 2,
				improvementStatuses: ['complete'],
				outcomeStatuses: ['failed', 'abort'],
				outcomeMeasure: 'count',
				levelOrder: 'mission',
				selectedSessionId: ''
			})
		);

		render(StatisticsPage);

		await waitFor(() => expect(mocks.getStatistics).toHaveBeenCalled());
		expect(screen.getByRole('tab', { name: 'Outcomes' })).toHaveAttribute('aria-selected', 'true');
		expect(mocks.getStatistics.mock.calls.at(-1)?.[0]).toMatchObject({
			bucket: 'month',
			levelNumber: 7,
			difficultyNumber: 2
		});
		expect(await screen.findByRole('radio', { name: 'Count' })).toHaveAttribute('aria-checked', 'true');
	});

	it('gives URL filters precedence over stored values', async () => {
		mocks.pageUrl = new URL('http://localhost/statistics?tab=improvement&range=7d&bucket=day&level=2&difficulty=1');
		localStorage.setItem(
			STATISTICS_PREFERENCES_STORAGE_KEY,
			JSON.stringify({
				version: 1,
				tab: 'outcomes',
				range: { preset: '12m', customFrom: '2026-01-01', customTo: '2026-07-24' },
				bucket: 'month',
				levelNumber: 7,
				difficultyNumber: 2
			})
		);

		render(StatisticsPage);

		await waitFor(() => expect(mocks.getStatistics).toHaveBeenCalled());
		expect(screen.getByRole('tab', { name: 'Improvement' })).toHaveAttribute('aria-selected', 'true');
		expect(mocks.getStatistics.mock.calls.at(-1)?.[0]).toMatchObject({
			bucket: 'day',
			levelNumber: 2,
			difficultyNumber: 1
		});
	});
});
