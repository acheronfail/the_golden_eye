import type { BarChartSeries, ChartData, ChartPoint, ChartSeries, LineChartSeries, XValue } from './Chart';

export interface ChartMargin {
	top: number;
	right: number;
	bottom: number;
	left: number;
}

export interface PointMark {
	series: ChartSeries;
	point: ChartPoint;
	x: number;
	y: number;
}

export interface BarMark extends PointMark {
	width: number;
	height: number;
}

export interface LineSeriesGeometry {
	series: LineChartSeries;
	path: string;
	points: PointMark[];
}

export interface CategoryLabel {
	value: XValue;
	x: number;
	y: number;
}

export interface ChartMarkInteractions {
	focus: (series: ChartSeries, point: ChartPoint) => void;
	blur: () => void;
	pointer: (event: PointerEvent, series: ChartSeries, point: ChartPoint) => void;
	pointerLeave: () => void;
	keydown: (event: KeyboardEvent, series: ChartSeries, point: ChartPoint) => void;
}

export interface ChartGeometry {
	kind: ChartData['kind'];
	width: number;
	height: number;
	margin: ChartMargin;
	plotWidth: number;
	plotHeight: number;
	categories: XValue[];
	minValue: number;
	maxValue: number;
	yTicks: number[];
	xTicks: XValue[];
	lineSeries: LineSeriesGeometry[];
	bars: BarMark[];
	categoryLabels: CategoryLabel[];
	xPosition: (value: XValue) => number;
	yPosition: (value: number) => number;
}

interface GeometryOptions {
	data: ChartData;
	visibleSeriesIds: Set<string>;
	width: number;
	yLabel: string;
	includeZero: boolean;
}

function chartCategories(data: ChartData): XValue[] {
	const values: XValue[] = [];
	for (const series of data.series) {
		for (const point of series.points) {
			if (!values.includes(point.x)) values.push(point.x);
		}
	}
	return data.xType === 'time' ? values.sort((a, b) => Number(a) - Number(b)) : values;
}

const isHorizontal = (data: ChartData): boolean =>
	data.kind === 'horizontalStackedBar' || data.kind === 'horizontalGroupedBar';

function smoothLinePath(points: Array<{ x: number; y: number }>): string {
	if (points.length === 0) return '';
	if (points.length === 1) return `M ${points[0].x} ${points[0].y}`;

	const slopes = points.slice(1).map((point, index) => {
		const previous = points[index];
		return (point.y - previous.y) / (point.x - previous.x);
	});
	const tangents = points.map((_, index) => {
		if (index === 0) return slopes[0];
		if (index === points.length - 1) return slopes.at(-1)!;
		const before = slopes[index - 1];
		const after = slopes[index];
		if (before === 0 || after === 0 || Math.sign(before) !== Math.sign(after)) return 0;
		return (2 * before * after) / (before + after);
	});

	return points.slice(1).reduce((path, point, index) => {
		const previous = points[index];
		const width = point.x - previous.x;
		const controlOffset = width / 3;
		return `${path} C ${previous.x + controlOffset} ${previous.y + tangents[index] * controlOffset} ${point.x - controlOffset} ${point.y - tangents[index + 1] * controlOffset} ${point.x} ${point.y}`;
	}, `M ${points[0].x} ${points[0].y}`);
}

function linePath(
	series: LineChartSeries,
	categories: XValue[],
	xPosition: (value: XValue) => number,
	yPosition: (value: number) => number
): string {
	const points = [...series.points].sort((a, b) => Number(a.x) - Number(b.x));
	if (series.lineStyle === 'step') {
		const path = points
			.map((point, index) => {
				if (index === 0) return `M ${xPosition(point.x)} ${yPosition(point.y)}`;
				return `H ${xPosition(point.x)} V ${yPosition(point.y)}`;
			})
			.join(' ');
		const lastCategory = categories.at(-1);
		const lastPoint = points.at(-1);
		return lastCategory != null && lastPoint != null && Number(lastCategory) > Number(lastPoint.x)
			? `${path} H ${xPosition(lastCategory)}`
			: path;
	}
	if (series.lineStyle === 'smooth') {
		return smoothLinePath(
			points.map((point) => ({
				x: xPosition(point.x),
				y: yPosition(point.y)
			}))
		);
	}
	return points
		.map((point, index) => `${index === 0 ? 'M' : 'L'} ${xPosition(point.x)} ${yPosition(point.y)}`)
		.join(' ');
}

function tickValues(
	data: ChartData,
	categories: XValue[],
	plotWidth: number,
	xPosition: (value: XValue) => number
): XValue[] {
	if (isHorizontal(data)) return [];
	if (data.xType !== 'time' || data.kind !== 'line') {
		const step = Math.max(1, Math.ceil(categories.length / Math.max(1, Math.floor(plotWidth / 70))));
		return categories.filter((_, index) => index % step === 0);
	}

	const minimumGap = 72;
	const ticks: XValue[] = [];
	for (const category of categories) {
		if (ticks.length === 0 || xPosition(category) - xPosition(ticks[ticks.length - 1]) >= minimumGap) {
			ticks.push(category);
		}
	}
	const last = categories.at(-1);
	if (last !== undefined && ticks.at(-1) !== last) {
		if (ticks.length > 1 && xPosition(last) - xPosition(ticks[ticks.length - 1]) < minimumGap) ticks.pop();
		ticks.push(last);
	}
	return ticks;
}

function stackTotal(series: ChartSeries[], category: XValue): number {
	return series.reduce((sum, candidate) => sum + (candidate.points.find((point) => point.x === category)?.y ?? 0), 0);
}

function stackOffset(series: ChartSeries[], seriesIndex: number, category: XValue): number {
	return stackTotal(series.slice(0, seriesIndex), category);
}

export function buildChartGeometry({
	data,
	visibleSeriesIds,
	width,
	yLabel,
	includeZero
}: GeometryOptions): ChartGeometry {
	const categories = chartCategories(data);
	const margin: ChartMargin = isHorizontal(data)
		? { top: 22, right: 16, bottom: 60, left: 96 }
		: { top: 22, right: 16, bottom: 60, left: yLabel ? 58 : 40 };
	const horizontalRowHeight = data.kind === 'horizontalGroupedBar' ? data.series.length * 12 + 10 : 25;
	const height = isHorizontal(data)
		? Math.max(320, categories.length * horizontalRowHeight + margin.top + margin.bottom)
		: 320;
	const plotWidth = Math.max(1, width - margin.left - margin.right);
	const plotHeight = height - margin.top - margin.bottom;
	const visibleSeries = data.series.filter((series) => visibleSeriesIds.has(series.id));
	const allPoints = visibleSeries.flatMap((series) => series.points);
	const rawMinValue = allPoints.length > 0 ? Math.min(...allPoints.map((point) => point.y)) : 0;
	const rawMaxValue = allPoints.length > 0 ? Math.max(...allPoints.map((point) => point.y)) : 1;
	const fittedPadding = Math.max(1, (rawMaxValue - rawMinValue) * 0.08);
	const minValue = data.kind === 'line' && !includeZero ? Math.max(0, rawMinValue - fittedPadding) : 0;
	const maxValue =
		data.kind === 'stackedBar' || data.kind === 'horizontalStackedBar'
			? Math.max(1, ...categories.map((category) => stackTotal(visibleSeries, category)))
			: data.kind === 'line' && !includeZero
				? rawMaxValue + fittedPadding
				: Math.max(1, rawMaxValue);
	const xPosition = (value: XValue): number => {
		if (data.xType === 'time' && data.kind === 'line') {
			const values = categories.map(Number);
			const min = Math.min(...values);
			const max = Math.max(...values);
			if (min === max) return margin.left + plotWidth / 2;
			const pointPadding = 6;
			return margin.left + pointPadding + ((Number(value) - min) / (max - min)) * (plotWidth - pointPadding * 2);
		}
		const index = categories.indexOf(value);
		return margin.left + ((index + 0.5) / Math.max(1, categories.length)) * plotWidth;
	};
	const yPosition = (value: number): number =>
		margin.top + plotHeight - ((value - minValue) / Math.max(1, maxValue - minValue)) * plotHeight;
	const yTicks = Array.from({ length: 5 }, (_, index) => minValue + ((maxValue - minValue) * index) / 4);

	const lineSeries: LineSeriesGeometry[] = [];
	const bars: BarMark[] = [];
	const categoryLabels: CategoryLabel[] = [];

	if (data.kind === 'line') {
		const series = data.series
			.filter((candidate) => visibleSeriesIds.has(candidate.id))
			.map((candidate, index) => ({ candidate, index }))
			.sort((a, b) => (a.candidate.renderPriority ?? 0) - (b.candidate.renderPriority ?? 0) || a.index - b.index)
			.map(({ candidate }) => candidate);
		for (const candidate of series) {
			lineSeries.push({
				series: candidate,
				path: linePath(candidate, categories, xPosition, yPosition),
				points: candidate.points.map((point) => ({
					series: candidate,
					point,
					x: xPosition(point.x),
					y: yPosition(point.y)
				}))
			});
		}
	} else if (isHorizontal(data)) {
		const series = data.series.filter((candidate) => visibleSeriesIds.has(candidate.id));
		const rowHeight = plotHeight / Math.max(1, categories.length);
		for (const [categoryIndex, category] of categories.entries()) {
			const grouped = data.kind === 'horizontalGroupedBar';
			const groupTop = margin.top + categoryIndex * rowHeight + rowHeight * 0.12;
			const groupHeight = rowHeight * 0.76;
			const barHeight = grouped ? groupHeight / Math.max(1, series.length) : groupHeight;
			categoryLabels.push({
				value: category,
				x: margin.left - 8,
				y: groupTop + groupHeight / 2 + 4
			});
			for (const [seriesIndex, candidate] of series.entries()) {
				const point = candidate.points.find((item) => item.x === category);
				if (!point || point.y <= 0) continue;
				const offset = grouped ? 0 : stackOffset(series, seriesIndex, category);
				const x = margin.left + (offset / maxValue) * plotWidth;
				const markWidth = (point.y / maxValue) * plotWidth;
				bars.push({
					series: candidate,
					point,
					x,
					y: grouped ? groupTop + seriesIndex * barHeight : groupTop,
					width: markWidth,
					height: Math.max(1, barHeight - (grouped ? 2 : 0))
				});
			}
		}
	} else {
		const series: BarChartSeries[] = data.series.filter((candidate) => visibleSeriesIds.has(candidate.id));
		const stacked = data.kind === 'stackedBar';
		const band = (plotWidth / Math.max(1, categories.length)) * 0.76;
		for (const category of categories) {
			const groupX = xPosition(category) - band / 2;
			for (const [seriesIndex, candidate] of series.entries()) {
				const point = candidate.points.find((item) => item.x === category);
				if (!point) continue;
				const offset = stackOffset(series, seriesIndex, category);
				const barWidth = stacked ? band : band / Math.max(1, series.length);
				const top = stacked ? yPosition(offset + point.y) : yPosition(point.y);
				const bottom = stacked ? yPosition(offset) : yPosition(0);
				bars.push({
					series: candidate,
					point,
					x: stacked ? groupX : groupX + seriesIndex * barWidth,
					y: top,
					width: Math.max(1, barWidth - 2),
					height: Math.max(0, bottom - top)
				});
			}
		}
	}

	return {
		kind: data.kind,
		width,
		height,
		margin,
		plotWidth,
		plotHeight,
		categories,
		minValue,
		maxValue,
		yTicks,
		xTicks: tickValues(data, categories, plotWidth, xPosition),
		lineSeries,
		bars,
		categoryLabels,
		xPosition,
		yPosition
	};
}

export function pointAnchor(geometry: ChartGeometry, series: ChartSeries, point: ChartPoint): PointMark | undefined {
	const linePoint = geometry.lineSeries
		.flatMap((candidate) => candidate.points)
		.find((mark) => mark.series.id === series.id && mark.point === point);
	if (linePoint) return linePoint;
	const bar = geometry.bars.find((mark) => mark.series.id === series.id && mark.point === point);
	return bar
		? {
				series,
				point,
				x: bar.x + bar.width / 2,
				y:
					geometry.kind === 'horizontalStackedBar' || geometry.kind === 'horizontalGroupedBar'
						? bar.y + bar.height / 2
						: bar.y
			}
		: undefined;
}
