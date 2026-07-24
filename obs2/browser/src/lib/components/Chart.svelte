<script lang="ts">
	import { onMount, tick } from 'svelte';
	import type { ChartData, ChartPoint, ChartSeries, XValue } from './Chart';

	let {
		data,
		title,
		description,
		xLabel = '',
		yLabel = '',
		formatXValue,
		formatValue = (value: number) => String(Math.round(value))
	}: {
		data: ChartData;
		title: string;
		description: string;
		xLabel?: string;
		yLabel?: string;
		formatXValue?: (value: XValue) => string;
		formatValue?: (value: number) => string;
	} = $props();

	const uid = $props.id();
	let container: HTMLDivElement;
	let svg = $state<SVGSVGElement>();
	let width = $state(720);
	let selected = $state<{ series: ChartSeries; point: ChartPoint; x: number; y: number } | null>(null);
	let tooltipContent = $state<SVGGElement>();
	let tooltipWidth = $state(120);
	let tooltipHideTimer: number | null = null;
	const margin = $derived(
		data.kind === 'horizontalStackedBar'
			? { top: 22, right: 16, bottom: 60, left: 96 }
			: { top: 22, right: 16, bottom: 60, left: yLabel ? 58 : 40 }
	);
	const plotWidth = $derived(Math.max(1, width - margin.left - margin.right));
	const visibleSeries = $derived(data.series.filter((series) => series.points.length > 0));
	const categories = $derived.by(() => {
		const values: XValue[] = [];
		for (const series of visibleSeries) {
			for (const point of series.points) {
				if (!values.includes(point.x)) values.push(point.x);
			}
		}
		return data.xType === 'time' ? values.sort((a, b) => Number(a) - Number(b)) : values;
	});
	const height = $derived(
		data.kind === 'horizontalStackedBar' ? Math.max(320, categories.length * 25 + margin.top + margin.bottom) : 320
	);
	const plotHeight = $derived(height - margin.top - margin.bottom);
	const allPoints = $derived(visibleSeries.flatMap((series) => series.points));
	const maxValue = $derived.by(() => {
		if (data.kind === 'stackedBar' || data.kind === 'horizontalStackedBar') {
			return Math.max(
				1,
				...categories.map((x) =>
					visibleSeries.reduce((sum, series) => sum + (series.points.find((point) => point.x === x)?.y ?? 0), 0)
				)
			);
		}
		return Math.max(1, ...allPoints.map((point) => point.y));
	});
	const yTicks = $derived(Array.from({ length: 5 }, (_, index) => (maxValue * index) / 4));

	$effect(() => {
		const selection = selected;
		if (!selection) return;
		void tick().then(() => {
			if (selected !== selection || !tooltipContent) return;
			const contentWidth = Math.ceil(tooltipContent.getBBox().width) + 16;
			tooltipWidth = Math.max(64, Math.min(plotWidth + 8, contentWidth));
		});
	});

	onMount(() => {
		const update = () => (width = Math.max(320, Math.round(container.clientWidth)));
		update();
		const observer = new ResizeObserver(update);
		observer.observe(container);
		return () => {
			observer.disconnect();
			if (tooltipHideTimer !== null) window.clearTimeout(tooltipHideTimer);
		};
	});

	function keepTooltipOpen() {
		if (tooltipHideTimer === null) return;
		window.clearTimeout(tooltipHideTimer);
		tooltipHideTimer = null;
	}

	function scheduleTooltipClose() {
		keepTooltipOpen();
		tooltipHideTimer = window.setTimeout(() => {
			selected = null;
			tooltipHideTimer = null;
		}, 120);
	}

	function xPosition(x: XValue): number {
		if (data.xType === 'time') {
			if (data.kind !== 'line') {
				const index = categories.indexOf(x);
				return margin.left + ((index + 0.5) / Math.max(1, categories.length)) * plotWidth;
			}
			const values = categories.map(Number);
			const min = Math.min(...values);
			const max = Math.max(...values);
			if (min === max) return margin.left + plotWidth / 2;
			const pointPadding = 6;
			return margin.left + pointPadding + ((Number(x) - min) / (max - min)) * (plotWidth - pointPadding * 2);
		}
		const index = categories.indexOf(x);
		return margin.left + ((index + 0.5) / Math.max(1, categories.length)) * plotWidth;
	}

	function yPosition(y: number): number {
		return margin.top + plotHeight - (y / maxValue) * plotHeight;
	}

	function categoryBand(): number {
		return plotWidth / Math.max(1, categories.length);
	}

	function linePath(series: ChartSeries): string {
		return [...series.points]
			.sort((a, b) => Number(a.x) - Number(b.x))
			.map((point, index) => `${index === 0 ? 'M' : 'L'} ${xPosition(point.x)} ${yPosition(point.y)}`)
			.join(' ');
	}

	function fillFor(series: ChartSeries): string {
		return series.pattern && series.pattern !== 'plain'
			? `url(#${uid}-${series.id}-${series.pattern})`
			: (series.surfaceColor ?? series.color);
	}

	function tickLabel(value: XValue): string {
		if (formatXValue) return formatXValue(value);
		if (data.xType === 'time') {
			return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(new Date(Number(value)));
		}
		return String(value);
	}

	function xTickCategories(): XValue[] {
		if (data.kind === 'horizontalStackedBar') return [];
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
		const last = categories[categories.length - 1];
		if (last !== undefined && ticks[ticks.length - 1] !== last) {
			if (ticks.length > 1 && xPosition(last) - xPosition(ticks[ticks.length - 1]) < minimumGap) ticks.pop();
			ticks.push(last);
		}
		return ticks;
	}

	function pointKeydown(event: KeyboardEvent, series: ChartSeries, point: ChartPoint) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			showTooltipAtPoint(series, point);
		}
		if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
			event.preventDefault();
			const points = visibleSeries.flatMap((candidateSeries) =>
				candidateSeries.points.map((candidatePoint) => ({
					series: candidateSeries,
					point: candidatePoint
				}))
			);
			const index = points.findIndex((candidate) => candidate.series.id === series.id && candidate.point === point);
			const offset = event.key === 'ArrowLeft' ? -1 : 1;
			const next = points[Math.max(0, Math.min(points.length - 1, index + offset))];
			if (next) showTooltipAtPoint(next.series, next.point);
		}
		if (event.key === 'Escape') selected = null;
	}

	function showTooltipAtPoint(series: ChartSeries, point: ChartPoint) {
		keepTooltipOpen();
		const categoryIndex = categories.indexOf(point.x);
		const seriesIndex = visibleSeries.indexOf(series);
		if (data.kind === 'horizontalStackedBar') {
			const rowHeight = plotHeight / Math.max(1, categories.length);
			const offset = visibleSeries
				.slice(0, seriesIndex)
				.reduce((sum, candidate) => sum + (candidate.points.find((item) => item.x === point.x)?.y ?? 0), 0);
			selected = {
				series,
				point,
				x: margin.left + ((offset + point.y / 2) / maxValue) * plotWidth,
				y: margin.top + (categoryIndex + 0.5) * rowHeight
			};
			return;
		}
		if (data.kind === 'stackedBar' || data.kind === 'groupedBar') {
			const band = categoryBand() * 0.76;
			const stacked = data.kind === 'stackedBar';
			const offset = visibleSeries
				.slice(0, seriesIndex)
				.reduce((sum, candidate) => sum + (candidate.points.find((item) => item.x === point.x)?.y ?? 0), 0);
			const barWidth = stacked ? band : band / visibleSeries.length;
			selected = {
				series,
				point,
				x: xPosition(point.x) - band / 2 + (stacked ? 0 : seriesIndex * barWidth) + barWidth / 2,
				y: stacked ? yPosition(offset + point.y) : yPosition(point.y)
			};
			return;
		}
		selected = { series, point, x: xPosition(point.x), y: yPosition(point.y) };
	}

	function showTooltipAtPointer(event: PointerEvent, series: ChartSeries, point: ChartPoint) {
		if (!svg) return;
		keepTooltipOpen();
		const bounds = svg.getBoundingClientRect();
		selected = {
			series,
			point,
			x: ((event.clientX - bounds.left) / bounds.width) * width,
			y: ((event.clientY - bounds.top) / bounds.height) * height
		};
	}
</script>

<figure class="m-0 min-w-0" aria-label={title}>
	<div bind:this={container} class="w-full min-w-0 overflow-hidden rounded-sm">
		{#if categories.length === 0}
			<div class="flex min-h-52 items-center justify-center rounded-sm obs-empty-state px-4 text-sm obs-muted">
				No data in this range
			</div>
		{:else}
			<svg
				bind:this={svg}
				class="block h-auto w-full overflow-hidden"
				viewBox={`0 0 ${width} ${height}`}
				role="img"
				aria-labelledby={`${uid}-title ${uid}-desc`}
			>
				<title id={`${uid}-title`}>{title}</title>
				<desc id={`${uid}-desc`}>{description}</desc>
				<defs>
					{#each visibleSeries as series}
						<pattern id={`${uid}-${series.id}-diagonal`} width="4" height="4" patternUnits="userSpaceOnUse">
							<rect width="4" height="4" fill={series.surfaceColor ?? series.color} />
							<path d="M-1 1 L1 -1 M0 4 L4 0 M3 5 L5 3" stroke={series.color} stroke-width="1" />
						</pattern>
						<pattern id={`${uid}-${series.id}-dots`} width="4" height="4" patternUnits="userSpaceOnUse">
							<rect width="4" height="4" fill={series.surfaceColor ?? series.color} />
							<circle cx="2" cy="2" r="0.7" fill={series.color} />
						</pattern>
					{/each}
				</defs>
				<rect
					x="0.5"
					y="0.5"
					width={width - 1}
					height={height - 1}
					rx="3"
					fill="var(--obs-bg)"
					stroke="var(--obs-border-soft)"
					stroke-width="1"
				/>

				{#each yTicks as tick}
					{#if data.kind === 'horizontalStackedBar'}
						<line
							x1={margin.left + (tick / maxValue) * plotWidth}
							x2={margin.left + (tick / maxValue) * plotWidth}
							y1={margin.top}
							y2={margin.top + plotHeight}
							stroke="var(--obs-border-muted)"
							stroke-width="1"
						/>
						<text
							x={margin.left + (tick / maxValue) * plotWidth}
							y={height - margin.bottom + 20}
							text-anchor="middle"
							fill="var(--obs-text-dim)"
							font-size="11">{formatValue(tick)}</text
						>
					{:else}
						<line
							x1={margin.left}
							x2={width - margin.right}
							y1={yPosition(tick)}
							y2={yPosition(tick)}
							stroke="var(--obs-border-muted)"
							stroke-width="1"
						/>
						<text
							x={margin.left - 10}
							y={yPosition(tick) + 4}
							text-anchor="end"
							fill="var(--obs-text-dim)"
							font-size="11">{formatValue(tick)}</text
						>
					{/if}
				{/each}

				{#if data.kind === 'line'}
					{#each visibleSeries as series}
						{#if series.points.length > 1}
							<path d={linePath(series)} fill="none" stroke={series.color} stroke-width="2" stroke-linejoin="round" />
						{/if}
						{#each series.points as point}
							<g
								tabindex="0"
								role="button"
								aria-label={`${series.label}: ${point.label ?? tickLabel(point.x)}, ${formatValue(point.y)}`}
								onfocus={() => showTooltipAtPoint(series, point)}
								onblur={() => (selected = null)}
								onpointerenter={(event) => showTooltipAtPointer(event, series, point)}
								onpointermove={(event) => showTooltipAtPointer(event, series, point)}
								onpointerleave={scheduleTooltipClose}
								onkeydown={(event) => pointKeydown(event, series, point)}
							>
								{#if series.shape === 'square'}
									<rect
										x={xPosition(point.x) - 4}
										y={yPosition(point.y) - 4}
										width="8"
										height="8"
										fill={series.surfaceColor ?? 'var(--obs-panel)'}
										stroke={series.color}
										stroke-width="2"
									/>
								{:else if series.shape === 'diamond'}
									<path
										d={`M ${xPosition(point.x)} ${yPosition(point.y) - 5} L ${xPosition(point.x) + 5} ${yPosition(point.y)} L ${xPosition(point.x)} ${yPosition(point.y) + 5} L ${xPosition(point.x) - 5} ${yPosition(point.y)} Z`}
										fill={series.surfaceColor ?? 'var(--obs-panel)'}
										stroke={series.color}
										stroke-width="2"
									/>
								{:else if series.shape === 'triangle'}
									<path
										d={`M ${xPosition(point.x)} ${yPosition(point.y) - 5} L ${xPosition(point.x) + 5} ${yPosition(point.y) + 4} L ${xPosition(point.x) - 5} ${yPosition(point.y) + 4} Z`}
										fill={series.surfaceColor ?? 'var(--obs-panel)'}
										stroke={series.color}
										stroke-width="2"
									/>
								{:else}
									<circle
										cx={xPosition(point.x)}
										cy={yPosition(point.y)}
										r="4"
										fill={series.surfaceColor ?? 'var(--obs-panel)'}
										stroke={series.color}
										stroke-width="2"
									/>
								{/if}
							</g>
						{/each}
					{/each}
				{:else if data.kind === 'horizontalStackedBar'}
					{#each categories as category, categoryIndex}
						{@const rowHeight = plotHeight / Math.max(1, categories.length)}
						{@const usableWidth = plotWidth}
						{@const baseY = margin.top + categoryIndex * rowHeight + rowHeight * 0.18}
						{@const barHeight = rowHeight * 0.64}
						{@const offsets = visibleSeries.map((_, index) =>
							visibleSeries
								.slice(0, index)
								.reduce((sum, series) => sum + (series.points.find((point) => point.x === category)?.y ?? 0), 0)
						)}
						<text
							x={margin.left - 8}
							y={baseY + barHeight / 2 + 4}
							text-anchor="end"
							fill="var(--obs-text-muted)"
							font-size="11">{category}</text
						>
						{#each visibleSeries as series, seriesIndex}
							{@const point = series.points.find((candidate) => candidate.x === category)}
							{#if point && point.y > 0}
								<rect
									x={margin.left + (offsets[seriesIndex] / maxValue) * usableWidth}
									y={baseY}
									width={(point.y / maxValue) * usableWidth}
									height={barHeight}
									fill={fillFor(series)}
									stroke={series.color}
									stroke-width="1.5"
									tabindex="0"
									role="button"
									aria-label={`${category}, ${series.label}: ${formatValue(point.y)}`}
									onfocus={() => showTooltipAtPoint(series, point)}
									onblur={() => (selected = null)}
									onpointerenter={(event) => showTooltipAtPointer(event, series, point)}
									onpointermove={(event) => showTooltipAtPointer(event, series, point)}
									onpointerleave={scheduleTooltipClose}
									onkeydown={(event) => pointKeydown(event, series, point)}
								/>
							{/if}
						{/each}
					{/each}
				{:else}
					{#each categories as category}
						{@const band = categoryBand() * 0.76}
						{@const groupX = xPosition(category) - band / 2}
						{@const stacked = data.kind === 'stackedBar'}
						{@const offsets = visibleSeries.map((_, index) =>
							visibleSeries
								.slice(0, index)
								.reduce((sum, series) => sum + (series.points.find((point) => point.x === category)?.y ?? 0), 0)
						)}
						{#each visibleSeries as series, seriesIndex}
							{@const point = series.points.find((candidate) => candidate.x === category)}
							{#if point}
								{@const barWidth = stacked ? band : band / visibleSeries.length}
								{@const barX = stacked ? groupX : groupX + seriesIndex * barWidth}
								{@const top = stacked ? yPosition(offsets[seriesIndex] + point.y) : yPosition(point.y)}
								{@const bottom = stacked ? yPosition(offsets[seriesIndex]) : yPosition(0)}
								<rect
									x={barX}
									y={top}
									width={Math.max(1, barWidth - 2)}
									height={Math.max(0, bottom - top)}
									fill={fillFor(series)}
									stroke={series.color}
									stroke-width="1.5"
									tabindex="0"
									role="button"
									aria-label={`${tickLabel(category)}, ${series.label}: ${formatValue(point.y)}`}
									onfocus={() => showTooltipAtPoint(series, point)}
									onblur={() => (selected = null)}
									onpointerenter={(event) => showTooltipAtPointer(event, series, point)}
									onpointermove={(event) => showTooltipAtPointer(event, series, point)}
									onpointerleave={scheduleTooltipClose}
									onkeydown={(event) => pointKeydown(event, series, point)}
								/>
							{/if}
						{/each}
					{/each}
				{/if}

				{#if data.kind !== 'horizontalStackedBar'}
					{#each xTickCategories() as category}
						<text
							x={xPosition(category)}
							y={height - margin.bottom + 20}
							text-anchor="middle"
							fill="var(--obs-text-dim)"
							font-size="11">{tickLabel(category)}</text
						>
					{/each}
				{/if}
				<text
					x={margin.left + plotWidth / 2}
					y={height - 10}
					text-anchor="middle"
					fill="var(--obs-text-muted)"
					font-size="12">{xLabel}</text
				>
				<text
					x="16"
					y={margin.top + plotHeight / 2}
					text-anchor="middle"
					fill="var(--obs-text-muted)"
					font-size="12"
					transform={`rotate(-90 16 ${margin.top + plotHeight / 2})`}>{yLabel}</text
				>
				{#if selected}
					{@const tooltipHeight = selected.point.detail ? 66 : 52}
					{@const tooltipX = Math.max(
						margin.left - 4,
						Math.min(width - margin.right - tooltipWidth + 4, selected.x + 10)
					)}
					{@const tooltipY = Math.max(
						margin.top - 4,
						Math.min(height - margin.bottom - tooltipHeight + 4, selected.y - tooltipHeight - 10)
					)}
					<g
						data-chart-tooltip
						aria-hidden="true"
						role="presentation"
						transform={`translate(${tooltipX} ${tooltipY})`}
						onpointerenter={keepTooltipOpen}
						onpointerleave={scheduleTooltipClose}
					>
						<rect
							x="0"
							y="0"
							width={tooltipWidth}
							height={tooltipHeight}
							rx="3"
							fill="var(--obs-bg-elevated)"
							stroke="var(--obs-border-soft)"
						/>
						<g bind:this={tooltipContent}>
							<text x="8" y="16" fill={selected.series.color} font-family="monospace" font-size="11">
								{selected.series.label}
							</text>
							<text x="8" y="32" fill="var(--obs-text)" font-family="monospace" font-size="11">
								{selected.point.label ?? tickLabel(selected.point.x)} · {formatValue(selected.point.y)}
							</text>
							{#if selected.point.detail}
								<text x="8" y="49" fill="var(--obs-text-muted)" font-family="monospace" font-size="10">
									{selected.point.detail}
								</text>
							{/if}
						</g>
					</g>
				{/if}
			</svg>
		{/if}
	</div>

	{#if visibleSeries.length > 0}
		<ul class="mt-2 flex list-none flex-wrap gap-x-4 gap-y-2 p-0 text-xs obs-muted" aria-label="Chart legend">
			{#each visibleSeries as series}
				<li class="flex items-center gap-1.5">
					<svg class="h-4 w-4 overflow-hidden" viewBox="0 0 12 12" aria-hidden="true">
						<rect
							x="1"
							y="1"
							width="10"
							height="10"
							rx="1"
							fill={fillFor(series)}
							stroke={series.color}
							stroke-width="1"
						/>
					</svg>
					{series.label}
				</li>
			{/each}
		</ul>
	{/if}
</figure>
