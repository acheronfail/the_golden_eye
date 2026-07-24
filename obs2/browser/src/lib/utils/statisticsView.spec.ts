import { describe, expect, it } from 'vitest';
import { statisticsFixture } from '../../stories/statisticsFixtures';
import { attemptsByLevelData, formatDuration, outcomeData, runTimeData } from './statisticsView';

describe('statistics chart data', () => {
	it('uses the requested status patterns for attempts by level', () => {
		const data = attemptsByLevelData(statisticsFixture, 'attempts');
		expect(data.series.map((series) => [series.id, series.color, series.pattern])).toEqual([
			['complete', 'var(--obs-success)', 'plain'],
			['failed', 'var(--obs-danger)', 'plain'],
			['abort', 'var(--obs-danger)', 'diagonal'],
			['kia', 'var(--obs-danger)', 'dots']
		]);
	});

	it('normalizes outcome shares across selected statuses only', () => {
		const data = outcomeData(statisticsFixture.overallBuckets, ['abort', 'failed'], 'share');
		const failed = data.series.find((series) => series.id === 'failed')!;
		expect(failed.points[0].y).toBeCloseTo((7 / 16) * 100);
	});

	it('adds the completed running-best line', () => {
		const data = runTimeData(statisticsFixture, ['complete']);
		expect(data.series.find((series) => series.id === 'running-best')?.points.map((point) => point.y)).toEqual([
			76, 71
		]);
	});

	it('formats combined durations without wrapping at 24 hours', () => {
		expect(formatDuration(30 * 60 * 60 + 61)).toBe('30:01:01');
	});
});
