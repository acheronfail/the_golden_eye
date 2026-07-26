<script lang="ts">
	import type { BarChartSeries } from './Chart';
	import type { BarMark, CategoryLabel, ChartMarkInteractions } from './geometry';

	let {
		marks,
		categoryLabels,
		interactions,
		formatValue,
		fillFor
	}: {
		marks: BarMark[];
		categoryLabels: CategoryLabel[];
		interactions: ChartMarkInteractions;
		formatValue: (value: number) => string;
		fillFor: (series: BarChartSeries) => string;
	} = $props();
</script>

{#each categoryLabels as label}
	<text x={label.x} y={label.y} text-anchor="end" fill="var(--obs-text-muted)" font-size="11">{label.value}</text>
{/each}

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
		aria-label={`${point.x}, ${series.label}: ${formatValue(point.y)}`}
		onfocus={() => interactions.focus(series, point)}
		onblur={interactions.blur}
		onpointerenter={(event) => interactions.pointer(event, series, point)}
		onpointermove={(event) => interactions.pointer(event, series, point)}
		onpointerleave={interactions.pointerLeave}
		onkeydown={(event) => interactions.keydown(event, series, point)}
	/>
{/each}
