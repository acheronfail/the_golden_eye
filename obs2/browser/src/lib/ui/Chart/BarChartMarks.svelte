<script lang="ts">
	import type { BarChartSeries, XValue } from './Chart';
	import type { BarMark, ChartMarkInteractions } from './geometry';

	let {
		marks,
		interactions,
		tickLabel,
		formatValue,
		fillFor
	}: {
		marks: BarMark[];
		interactions: ChartMarkInteractions;
		tickLabel: (value: XValue) => string;
		formatValue: (value: number) => string;
		fillFor: (series: BarChartSeries) => string;
	} = $props();
</script>

{#each marks as mark}
	{@const series = mark.series as BarChartSeries}
	{@const point = mark.point}
	<rect
		data-chart-series={series.id}
		x={mark.x}
		y={mark.y}
		width={mark.width}
		height={mark.height}
		fill={fillFor(series)}
		stroke={series.color}
		stroke-width="1.5"
		tabindex="0"
		role="button"
		aria-label={`${tickLabel(point.x)}, ${series.label}: ${formatValue(point.y)}`}
		onfocus={() => interactions.focus(series, point)}
		onblur={interactions.blur}
		onpointerenter={(event) => interactions.pointer(event, series, point)}
		onpointermove={(event) => interactions.pointer(event, series, point)}
		onpointerleave={interactions.pointerLeave}
		onkeydown={(event) => interactions.keydown(event, series, point)}
	/>
{/each}
