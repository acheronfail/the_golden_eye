import { describe, expect, it } from 'vitest';
import { sessionDetailFixture, statisticsFixture } from '../../../stories/features/statistics/statisticsFixtures';
import {
	attemptsByLevelData,
	formatDuration,
	mostPlayedLevel,
	outcomeData,
	runTimeData,
	sessionAttemptsData
} from './statisticsView';

describe('statistics chart data', () => {
	it('shows three difficulty bars per level', () => {
		const data = attemptsByLevelData(statisticsFixture, 'attempts');
		expect(data.kind).toBe('horizontalGroupedBar');
		expect(data.series.map((series) => series.label)).toEqual(['Agent', 'Secret Agent', '00 Agent']);
		expect(data.series.map((series) => series.points[0].y)).toEqual([14, 15, 12]);
	});

	it('can rank and measure levels by time spent', () => {
		const data = attemptsByLevelData(statisticsFixture, 'attempts', 'time');
		expect(data.series[0].points.map((point) => point.x)).toEqual(['Facility', 'Frigate', 'Dam']);
		expect(data.series[2].points[0].y).toBe(1_650);
		expect(mostPlayedLevel(statisticsFixture)).toEqual({ value: 'Facility', detail: '00 Agent' });
	});

	it('normalizes outcome shares across selected statuses only', () => {
		const data = outcomeData(statisticsFixture.overallBuckets, ['abort', 'failed'], 'share');
		const failed = data.series.find((series) => series.id === 'failed')!;
		expect(data.series.map((series) => series.id)).toEqual(['complete', 'failed', 'abort', 'kia']);
		expect(failed.points[0].y).toBeCloseTo((7 / 16) * 100);
		expect(data.kind).toBe('stackedBar');
	});

	it('adds the completed running-best line', () => {
		const data = runTimeData(statisticsFixture);
		expect(data.series.find((series) => series.id === 'running-best')?.points.map((point) => point.y)).toEqual([
			76, 71
		]);
		expect(data.series.find((series) => series.id === 'running-best')?.lineStyle).toBe('step');
		expect(data.series.find((series) => series.id === 'running-best')?.renderPriority).toBe(1);
		expect(
			data.series.filter((series) => series.id !== 'running-best').every((series) => series.lineStyle === 'none')
		).toBe(true);
		expect(data.referenceLines).toEqual([
			{
				id: 'personal-best',
				value: 71,
				color: 'var(--obs-gold-hover)',
				seriesId: 'running-best'
			}
		]);
	});

	it('shows dates without placeholder times in improvement tooltips', () => {
		const data = runTimeData(statisticsFixture);
		const expectedLabels = new Set(
			statisticsFixture.selectedCohort!.runTimes.map((run) => new Date(run.completedAt).toLocaleDateString())
		);

		for (const point of data.series.flatMap((series) => series.points)) {
			expect(expectedLabels).toContain(point.label);
		}
	});

	it('lightly smooths session attempt lines', () => {
		const data = sessionAttemptsData(sessionDetailFixture);

		expect(data.series.every((series) => series.lineStyle === 'smooth')).toBe(true);
	});

	it('formats combined durations without wrapping at 24 hours', () => {
		expect(formatDuration(30 * 60 * 60 + 61)).toBe('30:01:01');
	});
});
