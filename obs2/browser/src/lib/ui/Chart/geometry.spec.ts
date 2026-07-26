import { describe, expect, it } from 'vitest';
import type { ChartData } from './Chart';
import { buildChartGeometry, pointAnchor } from './geometry';

function geometry(data: ChartData, includeZero = true) {
	return buildChartGeometry({
		data,
		visibleSeriesIds: new Set(data.series.map((series) => series.id)),
		width: 720,
		yLabel: '',
		includeZero
	});
}

describe('chart geometry', () => {
	it('orders line series by render priority and fits a non-zero domain', () => {
		const data = {
			kind: 'line',
			xType: 'time',
			series: [
				{
					id: 'best',
					label: 'Best',
					color: 'gold',
					renderPriority: 1,
					lineStyle: 'step',
					points: [
						{ x: 1, y: 80 },
						{ x: 3, y: 70 }
					]
				},
				{
					id: 'attempt',
					label: 'Attempt',
					color: 'green',
					points: [{ x: 2, y: 75 }]
				}
			]
		} satisfies ChartData;

		const result = geometry(data, false);

		expect(result.lineSeries.map(({ series }) => series.id)).toEqual(['attempt', 'best']);
		expect(result.minValue).toBeLessThan(70);
		expect(result.maxValue).toBeGreaterThan(80);
		expect(result.lineSeries[1].path).toContain('H');
	});

	it('uses one stack domain and the rendered bar geometry for tooltip anchors', () => {
		const data = {
			kind: 'stackedBar',
			xType: 'category',
			series: [
				{
					id: 'complete',
					label: 'Complete',
					color: 'green',
					points: [{ x: 'Dam', y: 4 }]
				},
				{
					id: 'failed',
					label: 'Failed',
					color: 'red',
					points: [{ x: 'Dam', y: 3 }]
				}
			]
		} satisfies ChartData;

		const result = geometry(data);
		const failed = result.bars[1];
		const anchor = pointAnchor(result, failed.series, failed.point);

		expect(result.maxValue).toBe(7);
		expect(result.bars[0].x).toBe(result.bars[1].x);
		expect(failed.y).toBe(result.yPosition(7));
		expect(anchor).toMatchObject({
			x: failed.x + failed.width / 2,
			y: failed.y
		});
	});

	it('places grouped bars side by side', () => {
		const data = {
			kind: 'groupedBar',
			xType: 'category',
			series: [
				{ id: 'a', label: 'A', color: 'green', points: [{ x: 'Dam', y: 4 }] },
				{ id: 'b', label: 'B', color: 'red', points: [{ x: 'Dam', y: 3 }] }
			]
		} satisfies ChartData;

		const result = geometry(data);

		expect(result.maxValue).toBe(4);
		expect(result.bars[0].x).toBeLessThan(result.bars[1].x);
		expect(result.bars[0].width).toBe(result.bars[1].width);
	});

	it('grows horizontal charts with their categories and omits zero-sized marks', () => {
		const categories = Array.from({ length: 10 }, (_, index) => `Level ${index + 1}`);
		const data = {
			kind: 'horizontalStackedBar',
			xType: 'category',
			series: [
				{
					id: 'complete',
					label: 'Complete',
					color: 'green',
					points: categories.map((x, index) => ({ x, y: index }))
				}
			]
		} satisfies ChartData;

		const result = geometry(data);
		const firstBar = result.bars[0];

		expect(result.height).toBe(332);
		expect(result.categoryLabels).toHaveLength(10);
		expect(result.bars).toHaveLength(9);
		expect(result.xTicks).toEqual([]);
		expect(pointAnchor(result, firstBar.series, firstBar.point)?.y).toBe(firstBar.y + firstBar.height / 2);
	});
});
