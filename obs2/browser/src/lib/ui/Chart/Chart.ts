export type XValue = number | string;
export type ChartPattern = 'plain' | 'diagonal' | 'dots';

export interface ChartPoint {
	x: XValue;
	y: number;
	label?: string;
	detail?: string;
}

export interface BaseChartSeries {
	id: string;
	label: string;
	points: ChartPoint[];
	color: string;
	surfaceColor?: string;
}

export interface LineChartSeries extends BaseChartSeries {
	shape?: 'circle' | 'square' | 'triangle';
	lineStyle?: 'linear' | 'step' | 'none';
	renderPriority?: number;
}

export interface BarChartSeries extends BaseChartSeries {
	pattern?: ChartPattern;
}

export type ChartSeries = LineChartSeries | BarChartSeries;

export interface ChartReferenceLine {
	id: string;
	value: number;
	color: string;
	seriesId?: string;
}

interface BaseChartData {
	xType: 'time' | 'category';
	referenceLines?: ChartReferenceLine[];
}

export interface LineChartData extends BaseChartData {
	kind: 'line';
	series: LineChartSeries[];
}

export interface VerticalBarChartData extends BaseChartData {
	kind: 'stackedBar' | 'groupedBar';
	series: BarChartSeries[];
}

export interface HorizontalStackedBarChartData extends BaseChartData {
	kind: 'horizontalStackedBar';
	xType: 'category';
	series: BarChartSeries[];
}

export type ChartData = LineChartData | VerticalBarChartData | HorizontalStackedBarChartData;
