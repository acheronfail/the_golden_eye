import { render, screen, waitFor } from '@testing-library/svelte';
import { SvelteURL } from 'svelte/reactivity';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { statisticsFixture } from '../../../stories/features/statistics/statisticsFixtures';
import { STATISTICS_PREFERENCES_STORAGE_KEY } from '$lib/features/statistics/statisticsPreferences';
import { completedRun } from '../../../stories/fixtures';
import StatisticsPage from './+page.svelte';

const mocks = vi.hoisted(() => ({
	getRuns: vi.fn(),
	getBestTimes: vi.fn(),
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
			getRuns: mocks.getRuns,
			getBestTimes: mocks.getBestTimes,
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
	mocks.pageUrl = new SvelteURL('http://localhost/statistics');
	mocks.getStatistics.mockResolvedValue(statisticsFixture);
	mocks.getStatisticsSessions.mockResolvedValue([]);
});

describe('/statistics', () => {
	it('loads only best times for the overview without requesting history or other statistics', async () => {
		mocks.pageUrl = new SvelteURL('http://localhost/statistics?tab=times');
		mocks.getBestTimes.mockResolvedValue([
			{ ...completedRun, path: '', metadata: { ...completedRun.metadata, timeSeconds: 45 } }
		]);
		render(StatisticsPage);
		expect(await screen.findByRole('link', { name: 'Facility 00 Agent 0:45 history' })).toHaveAttribute(
			'href',
			'/statistics?tab=times&timesLevel=2&timesDifficulty=00+Agent'
		);
		expect(mocks.getBestTimes).toHaveBeenCalledTimes(1);
		expect(mocks.getRuns).not.toHaveBeenCalled();
		expect(mocks.getStatistics).not.toHaveBeenCalled();
		expect(screen.queryByRole('combobox', { name: 'Group by' })).not.toBeInTheDocument();
	});

	it('requests older history only on demand and keeps loaded times after a page failure', async () => {
		const user = userEvent.setup();
		mocks.pageUrl = new SvelteURL('http://localhost/statistics?tab=times&timesLevel=2&timesDifficulty=00+Agent');
		mocks.getRuns
			.mockResolvedValueOnce({ clips: [completedRun], nextCursor: 'older' })
			.mockRejectedValueOnce(new Error('Connection lost'))
			.mockResolvedValueOnce({ clips: [{ ...completedRun, runId: 'older-run' }] });
		render(StatisticsPage);
		await screen.findByRole('link', { name: /Open run/ });
		expect(mocks.getRuns).toHaveBeenCalledTimes(1);
		expect(mocks.getRuns).toHaveBeenLastCalledWith(
			expect.objectContaining({
				limit: 50,
				cursor: undefined,
				filters: expect.objectContaining({
					level: 'Facility',
					difficulty: '00 Agent',
					status: 'complete',
					minTime: '0'
				})
			})
		);
		expect(mocks.getBestTimes).not.toHaveBeenCalled();
		expect(mocks.getStatistics).not.toHaveBeenCalled();
		await user.click(screen.getByRole('button', { name: 'Load more times' }));
		await screen.findByRole('alert');
		expect(screen.getAllByRole('link', { name: /Open run/ })).toHaveLength(1);
		await user.click(screen.getByRole('button', { name: 'Retry' }));
		await waitFor(() => expect(screen.getAllByRole('link', { name: /Open run/ })).toHaveLength(2));
		expect(mocks.getRuns).toHaveBeenLastCalledWith(expect.objectContaining({ cursor: 'older' }));
		expect(screen.queryByRole('button', { name: 'Load more times' })).not.toBeInTheDocument();
	});

	it('shows dates and systems in history and links to the run detail route', async () => {
		mocks.pageUrl = new SvelteURL('http://localhost/statistics?tab=times&timesLevel=2&timesDifficulty=00+Agent');
		mocks.getRuns.mockResolvedValue({
			clips: [{ ...completedRun, metadata: { ...completedRun.metadata, romVersion: 'pal' } }]
		});
		render(StatisticsPage);
		expect(await screen.findByRole('link', { name: /Open run 0:58 from/ })).toHaveAttribute(
			'href',
			'/runs?runId=completed-run'
		);
		expect(screen.getByText('PAL')).toBeInTheDocument();
		expect(screen.getByRole('link', { name: '← All best times' })).toHaveAttribute('href', '/statistics?tab=times');
	});

	it('cancels old history requests when navigating to another cohort or back to best times', async () => {
		mocks.pageUrl = new SvelteURL('http://localhost/statistics?tab=times&timesLevel=2&timesDifficulty=00+Agent');
		let finishOld!: (value: { clips: (typeof completedRun)[] }) => void;
		mocks.getRuns
			.mockImplementationOnce(
				() =>
					new Promise((resolve) => {
						finishOld = resolve;
					})
			)
			.mockResolvedValueOnce({ clips: [] });
		mocks.getBestTimes.mockResolvedValue([]);
		render(StatisticsPage);
		await waitFor(() => expect(mocks.getRuns).toHaveBeenCalledTimes(1));
		const signal = mocks.getRuns.mock.calls[0][0].signal as AbortSignal;
		mocks.pageUrl.search = '?tab=times&timesLevel=1&timesDifficulty=Agent';
		await waitFor(() => expect(mocks.getRuns).toHaveBeenCalledTimes(2));
		expect(signal.aborted).toBe(true);
		expect(mocks.getRuns).toHaveBeenLastCalledWith(
			expect.objectContaining({
				filters: expect.objectContaining({ level: 'Dam', difficulty: 'Agent' })
			})
		);
		finishOld({ clips: [completedRun] });
		await screen.findByText('No completed times for this level and difficulty yet.');
		expect(screen.queryByRole('link', { name: /Open run/ })).not.toBeInTheDocument();
		mocks.pageUrl.search = '?tab=times';
		await waitFor(() => expect(mocks.getBestTimes).toHaveBeenCalledTimes(1));
		expect(await screen.findByText('No completed times yet.')).toBeInTheDocument();
	});

	it('loads statistics after leaving Times and aborts them when returning', async () => {
		const user = userEvent.setup();
		mocks.pageUrl = new SvelteURL('http://localhost/statistics?tab=times');
		mocks.getBestTimes.mockResolvedValue([]);
		mocks.getStatistics.mockImplementation(() => new Promise(() => {}));
		render(StatisticsPage);
		await screen.findByText('No completed times yet.');
		expect(mocks.getStatistics).not.toHaveBeenCalled();
		await user.click(screen.getByRole('tab', { name: 'Overview' }));
		await waitFor(() => expect(mocks.getStatistics).toHaveBeenCalledTimes(1));
		const signal = mocks.getStatistics.mock.calls[0][1].signal as AbortSignal;
		await user.click(screen.getByRole('tab', { name: 'Times' }));
		expect(signal.aborted).toBe(true);
	});

	it('toggles difficulty series from the level chart legend', async () => {
		const user = userEvent.setup();
		render(StatisticsPage);

		expect(await screen.findByRole('button', { name: 'Hide Agent' })).toBeInTheDocument();
		await user.click(screen.getByRole('button', { name: 'Hide Agent' }));

		expect(screen.getByRole('button', { name: 'Show Agent' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Hide Secret Agent' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Hide 00 Agent' })).toBeInTheDocument();
	});

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
				levelDifficulties: [0, 2],
				attemptsOverTimeStatuses: ['complete', 'failed'],
				improvementSeries: ['running-best'],
				outcomeStatuses: ['failed', 'abort'],
				sessionStatuses: ['complete'],
				outcomeMeasure: 'count',
				levelMeasure: 'time',
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
		expect(screen.getByRole('button', { name: 'Hide Failed' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Show Complete' })).toBeInTheDocument();
	});

	it('gives URL filters precedence over stored values', async () => {
		mocks.pageUrl = new SvelteURL(
			'http://localhost/statistics?tab=improvement&range=7d&bucket=day&level=2&difficulty=1'
		);
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
		expect(await screen.findByRole('button', { name: 'Hide Personal best' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Hide Complete' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Show Failed' })).toBeInTheDocument();
		expect(screen.queryByRole('combobox', { name: 'Group by' })).not.toBeInTheDocument();
	});
});
