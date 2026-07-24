export type ChartKind = 'line' | 'stackedBar' | 'groupedBar' | 'horizontalStackedBar';
export type XValue = number | string;
export type ChartPattern = 'plain' | 'diagonal' | 'dots';

export interface ChartPoint {
	x: XValue;
	y: number;
	label?: string;
	detail?: string;
}

export interface ChartSeries {
	id: string;
	label: string;
	points: ChartPoint[];
	color: string;
	surfaceColor?: string;
	pattern?: ChartPattern;
	shape?: 'circle' | 'square' | 'triangle';
	lineStyle?: 'linear' | 'step' | 'none';
}

export interface ChartReferenceLine {
	id: string;
	value: number;
	color: string;
	seriesId?: string;
}

export interface ChartData {
	kind: ChartKind;
	series: ChartSeries[];
	xType: 'time' | 'category';
	referenceLines?: ChartReferenceLine[];
}
