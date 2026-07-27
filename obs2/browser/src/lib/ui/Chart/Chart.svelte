<script lang="ts">
	import { onMount, tick } from 'svelte';
	import BarChartMarks from './BarChartMarks.svelte';
	import HorizontalStackedBarMarks from './HorizontalStackedBarMarks.svelte';
	import LineChartMarks from './LineChartMarks.svelte';
	import type { BarChartSeries, ChartData, ChartPoint, ChartSeries, XValue } from './Chart';
	import { buildChartGeometry, pointAnchor, type ChartMarkInteractions } from './geometry';

	let {
		data,
		title,
		description,
		xLabel = '',
		yLabel = '',
		formatXValue,
		formatValue = (value: number) => String(Math.round(value)),
		includeZero = true,
		interactiveLegend = false,
		visibleSeriesIds,
		onVisibleSeriesChange
	}: {
		data: ChartData;
		title: string;
		description: string;
		xLabel?: string;
		yLabel?: string;
		formatXValue?: (value: XValue) => string;
		formatValue?: (value: number) => string;
		includeZero?: boolean;
		interactiveLegend?: boolean;
		visibleSeriesIds?: string[];
		onVisibleSeriesChange?: (ids: string[]) => void;
	} = $props();

	const uid = $props.id();
	let container: HTMLDivElement;
	let svg = $state<SVGSVGElement>();
	let width = $state(720);
	let selected = $state<{ series: ChartSeries; point: ChartPoint; x: number; y: number } | null>(null);
	let tooltipContent = $state<SVGGElement>();
	let tooltipWidth = $state(120);
	let tooltipHideTimer: number | null = null;
	const legendSeries = $derived(data.series);
	const visibleSeries = $derived(
		legendSeries.filter(
			(series) => !interactiveLegend || visibleSeriesIds == null || visibleSeriesIds.includes(series.id)
		)
	);
	const geometry = $derived(
		buildChartGeometry({
			data,
			visibleSeriesIds: new Set(visibleSeries.map((series) => series.id)),
			width,
			yLabel,
			includeZero
		})
	);

	$effect(() => {
		const selection = selected;
		if (!selection) return;
		void tick().then(() => {
			if (selected !== selection || !tooltipContent) return;
			const contentWidth = Math.ceil(tooltipContent.getBBox().width) + 16;
			tooltipWidth = Math.max(64, Math.min(geometry.plotWidth + 8, contentWidth));
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

	function toggleSeries(seriesId: string) {
		if (!interactiveLegend || !onVisibleSeriesChange) return;
		const current = visibleSeriesIds ?? legendSeries.map((series) => series.id);
		onVisibleSeriesChange(
			current.includes(seriesId) ? current.filter((id) => id !== seriesId) : [...current, seriesId]
		);
	}

	function fillFor(series: BarChartSeries): string {
		return series.pattern && series.pattern !== 'plain'
			? `url(#${uid}-${series.id}-${series.pattern})`
			: (series.surfaceColor ?? series.color);
	}

	function legendFillFor(series: ChartSeries): string {
		return 'pattern' in series ? fillFor(series) : (series.surfaceColor ?? series.color);
	}

	function tickLabel(value: XValue): string {
		if (formatXValue) return formatXValue(value);
		if (data.xType === 'time') {
			return new Intl.DateTimeFormat(undefined, { month: 'short', day: 'numeric' }).format(new Date(Number(value)));
		}
		return String(value);
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
		const anchor = pointAnchor(geometry, series, point);
		if (anchor) selected = anchor;
	}

	function showTooltipAtPointer(event: PointerEvent, series: ChartSeries, point: ChartPoint) {
		if (!svg) return;
		keepTooltipOpen();
		const bounds = svg.getBoundingClientRect();
		selected = {
			series,
			point,
			x: ((event.clientX - bounds.left) / bounds.width) * width,
			y: ((event.clientY - bounds.top) / bounds.height) * geometry.height
		};
	}

	const interactions: ChartMarkInteractions = {
		focus: showTooltipAtPoint,
		blur: () => (selected = null),
		pointer: showTooltipAtPointer,
		pointerLeave: scheduleTooltipClose,
		keydown: pointKeydown
	};
</script>

<figure class="m-0 min-w-0" aria-label={title}>
	<div bind:this={container} class="w-full min-w-0 overflow-hidden rounded-sm">
		{#if geometry.categories.length === 0}
			<div class="obs-preview-missing min-h-52 rounded-sm px-4">
				<span class="font-mono text-xs leading-snug">No data in this range</span>
			</div>
		{:else}
			<svg
				bind:this={svg}
				class="block h-auto w-full overflow-hidden"
				viewBox={`0 0 ${width} ${geometry.height}`}
				role="img"
				aria-label={title}
				aria-describedby={`${uid}-desc`}
			>
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
					height={geometry.height - 1}
					rx="3"
					fill="var(--obs-bg)"
					stroke="var(--obs-border-soft)"
					stroke-width="1"
				/>

				{#each geometry.yTicks as tick}
					{#if data.kind === 'horizontalStackedBar' || data.kind === 'horizontalGroupedBar'}
						<line
							x1={geometry.margin.left + (tick / geometry.maxValue) * geometry.plotWidth}
							x2={geometry.margin.left + (tick / geometry.maxValue) * geometry.plotWidth}
							y1={geometry.margin.top}
							y2={geometry.margin.top + geometry.plotHeight}
							stroke="var(--obs-border-muted)"
							stroke-width="1"
						/>
						<text
							x={geometry.margin.left + (tick / geometry.maxValue) * geometry.plotWidth}
							y={geometry.height - geometry.margin.bottom + 20}
							text-anchor="middle"
							fill="var(--obs-text-dim)"
							font-size="11">{formatValue(tick)}</text
						>
					{:else}
						<line
							x1={geometry.margin.left}
							x2={width - geometry.margin.right}
							y1={geometry.yPosition(tick)}
							y2={geometry.yPosition(tick)}
							stroke="var(--obs-border-muted)"
							stroke-width="1"
						/>
						<text
							x={geometry.margin.left - 10}
							y={geometry.yPosition(tick) + 4}
							text-anchor="end"
							fill="var(--obs-text-dim)"
							font-size="11">{formatValue(tick)}</text
						>
					{/if}
				{/each}

				{#if data.kind === 'line'}
					<LineChartMarks seriesGeometry={geometry.lineSeries} {interactions} {tickLabel} {formatValue} />
				{:else if data.kind === 'horizontalStackedBar' || data.kind === 'horizontalGroupedBar'}
					<HorizontalStackedBarMarks
						marks={geometry.bars}
						categoryLabels={geometry.categoryLabels}
						{interactions}
						{formatValue}
						{fillFor}
					/>
				{:else if data.kind === 'stackedBar' || data.kind === 'groupedBar'}
					<BarChartMarks marks={geometry.bars} {interactions} {tickLabel} {formatValue} {fillFor} />
				{/if}

				{#each data.referenceLines ?? [] as reference}
					{#if reference.seriesId == null || visibleSeries.some((series) => series.id === reference.seriesId)}
						<line
							data-chart-reference-line={reference.id}
							x1={geometry.margin.left}
							x2={width - geometry.margin.right}
							y1={geometry.yPosition(reference.value)}
							y2={geometry.yPosition(reference.value)}
							stroke={reference.color}
							stroke-width="1"
							stroke-dasharray="0.5 2.5"
							stroke-linecap="round"
							opacity="0.9"
							aria-hidden="true"
						/>
					{/if}
				{/each}

				{#if data.kind !== 'horizontalStackedBar' && data.kind !== 'horizontalGroupedBar'}
					{#each geometry.xTicks as category}
						<text
							x={geometry.xPosition(category)}
							y={geometry.height - geometry.margin.bottom + 20}
							text-anchor="middle"
							fill="var(--obs-text-dim)"
							font-size="11">{tickLabel(category)}</text
						>
					{/each}
				{/if}
				<text
					x={geometry.margin.left + geometry.plotWidth / 2}
					y={geometry.height - 10}
					text-anchor="middle"
					fill="var(--obs-text-muted)"
					font-size="12">{xLabel}</text
				>
				<text
					x="16"
					y={geometry.margin.top + geometry.plotHeight / 2}
					text-anchor="middle"
					fill="var(--obs-text-muted)"
					font-size="12"
					transform={`rotate(-90 16 ${geometry.margin.top + geometry.plotHeight / 2})`}>{yLabel}</text
				>
				{#if selected}
					{@const tooltipHeight = selected.point.detail ? 66 : 52}
					{@const tooltipX = Math.max(
						geometry.margin.left - 4,
						Math.min(width - geometry.margin.right - tooltipWidth + 4, selected.x + 10)
					)}
					{@const tooltipY = Math.max(
						geometry.margin.top - 4,
						Math.min(geometry.height - geometry.margin.bottom - tooltipHeight + 4, selected.y - tooltipHeight - 10)
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

	{#if legendSeries.length > 0}
		<ul class="mt-2 flex list-none flex-wrap gap-x-4 gap-y-2 p-0 text-xs obs-muted" aria-label="Chart legend">
			{#each legendSeries as series}
				{@const enabled = !interactiveLegend || visibleSeriesIds == null || visibleSeriesIds.includes(series.id)}
				<li>
					{#if interactiveLegend}
						<button
							type="button"
							class="flex items-center gap-1.5 rounded-sm transition-opacity hover:text-(--obs-text) focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-(--obs-gold-hover)"
							class:opacity-45={!enabled}
							class:line-through={!enabled}
							aria-pressed={enabled}
							aria-label={`${enabled ? 'Hide' : 'Show'} ${series.label}`}
							onclick={() => toggleSeries(series.id)}
						>
							<svg class="h-4 w-4 overflow-visible" viewBox="0 0 12 12" aria-hidden="true">
								{#if data.kind === 'line' && 'lineStyle' in series && series.lineStyle === 'step'}
									<path
										d="M1 9 H6 V3 H11"
										fill="none"
										stroke={enabled ? series.color : 'currentColor'}
										stroke-width="2"
									/>
								{:else if data.kind === 'line' && 'shape' in series && series.shape === 'square'}
									<rect x="3" y="3" width="6" height="6" fill={enabled ? series.color : 'currentColor'} />
								{:else if data.kind === 'line' && 'shape' in series && series.shape === 'triangle'}
									<path d="M6 2 L10 9 L2 9 Z" fill={enabled ? series.color : 'currentColor'} />
								{:else if data.kind === 'line'}
									<circle cx="6" cy="6" r="3" fill={enabled ? series.color : 'currentColor'} />
								{:else if data.kind === 'stackedBar' || data.kind === 'groupedBar' || data.kind === 'horizontalStackedBar' || data.kind === 'horizontalGroupedBar'}
									<rect
										x="1"
										y="1"
										width="10"
										height="10"
										rx="1"
										fill={enabled ? legendFillFor(series) : 'currentColor'}
										stroke={enabled ? series.color : 'currentColor'}
										stroke-width="1"
									/>
								{/if}
							</svg>
							{series.label}
						</button>
					{:else}
						<span class="flex items-center gap-1.5">
							<svg class="h-4 w-4 overflow-hidden" viewBox="0 0 12 12" aria-hidden="true">
								<rect
									x="1"
									y="1"
									width="10"
									height="10"
									rx="1"
									fill={legendFillFor(series)}
									stroke={series.color}
									stroke-width="1"
								/>
							</svg>
							{series.label}
						</span>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</figure>
