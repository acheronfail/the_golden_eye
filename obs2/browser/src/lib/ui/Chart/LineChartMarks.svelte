<script lang="ts">
	import type { ChartMarkInteractions, LineSeriesGeometry } from './geometry';
	import type { XValue } from './Chart';

	let {
		seriesGeometry,
		interactions,
		tickLabel,
		formatValue
	}: {
		seriesGeometry: LineSeriesGeometry[];
		interactions: ChartMarkInteractions;
		tickLabel: (value: XValue) => string;
		formatValue: (value: number) => string;
	} = $props();
</script>

{#each seriesGeometry as geometry}
	{@const series = geometry.series}
	{#if series.lineStyle !== 'none' && series.points.length > 1}
		<path
			data-chart-series={series.id}
			d={geometry.path}
			fill="none"
			stroke={series.color}
			stroke-width="2"
			stroke-linejoin="round"
		/>
	{/if}
	{#each geometry.points as mark}
		{@const point = mark.point}
		<g
			data-chart-series={series.id}
			tabindex="0"
			role="button"
			aria-label={`${series.label}: ${point.label ?? tickLabel(point.x)}, ${formatValue(point.y)}`}
			onfocus={() => interactions.focus(series, point)}
			onblur={interactions.blur}
			onpointerenter={(event) => interactions.pointer(event, series, point)}
			onpointermove={(event) => interactions.pointer(event, series, point)}
			onpointerleave={interactions.pointerLeave}
			onkeydown={(event) => interactions.keydown(event, series, point)}
		>
			{#if series.shape === 'square'}
				<rect
					x={mark.x - 4}
					y={mark.y - 4}
					width="8"
					height="8"
					fill={series.surfaceColor ?? 'var(--obs-panel)'}
					stroke={series.color}
					stroke-width="2"
				/>
			{:else if series.shape === 'triangle'}
				<path
					d={`M ${mark.x} ${mark.y - 5} L ${mark.x + 5} ${mark.y + 4} L ${mark.x - 5} ${mark.y + 4} Z`}
					fill={series.surfaceColor ?? 'var(--obs-panel)'}
					stroke={series.color}
					stroke-width="2"
				/>
			{:else}
				<circle
					cx={mark.x}
					cy={mark.y}
					r="4"
					fill={series.surfaceColor ?? 'var(--obs-panel)'}
					stroke={series.color}
					stroke-width="2"
				/>
			{/if}
		</g>
	{/each}
{/each}
