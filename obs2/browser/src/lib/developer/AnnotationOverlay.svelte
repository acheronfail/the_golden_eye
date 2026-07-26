<script lang="ts">
	import type { AnnotationSet } from '$lib/api';
	import Select from '$lib/components/Select.svelte';
	import {
		annotationListItems,
		LABEL_LINE_HEIGHT,
		LABEL_PADDING_X,
		LABEL_PADDING_Y,
		placeAnnotations
	} from './annotationLayout';

	let {
		imageData,
		annotationSets,
		frameWidth,
		frameHeight,
		selectedAnnotationSetId = $bindable(null),
		hiddenAnnotationIds = $bindable([])
	}: {
		imageData: string;
		annotationSets: AnnotationSet[];
		frameWidth: number;
		frameHeight: number;
		selectedAnnotationSetId: string | null;
		hiddenAnnotationIds: string[];
	} = $props();

	const uid = $props.id();
	const annotationSetOptions = $derived(annotationSets.map((set) => ({ value: set.id, label: set.label })));
	const selectedAnnotationSet = $derived(
		annotationSets.find((set) => set.id === selectedAnnotationSetId) ?? annotationSets[0] ?? null
	);
	const annotationItems = $derived(annotationListItems(selectedAnnotationSet));
	const visibleAnnotationItems = $derived(annotationItems.filter((item) => !hiddenAnnotationIds.includes(item.id)));
	const placedAnnotations = $derived(placeAnnotations(visibleAnnotationItems, frameWidth, frameHeight));

	const toggleAnnotation = (id: string) => {
		hiddenAnnotationIds = hiddenAnnotationIds.includes(id)
			? hiddenAnnotationIds.filter((item) => item !== id)
			: [...hiddenAnnotationIds, id];
	};
</script>

{#if selectedAnnotationSet}
	<div class="flex min-w-0 flex-col gap-2">
		<label class="grid gap-1 text-sm sm:max-w-72">
			<span class="obs-muted">Annotation set</span>
			<Select
				class="w-full text-sm"
				value={selectedAnnotationSet.id}
				onChange={(value) => (selectedAnnotationSetId = value)}
				options={annotationSetOptions}
			/>
		</label>
		<div class="grid gap-2 text-sm">
			<div class="flex items-center justify-between gap-3">
				<span class="obs-muted">Visible annotations</span>
				<span class="font-mono text-xs obs-dim">{visibleAnnotationItems.length}/{annotationItems.length}</span>
			</div>
			<div class="grid max-h-36 gap-1 overflow-auto pr-1 sm:grid-cols-2">
				{#each annotationItems as item}
					<label class="flex min-w-0 items-center gap-2 rounded px-1 py-0.5">
						<input
							type="checkbox"
							class="obs-checkbox shrink-0"
							checked={!hiddenAnnotationIds.includes(item.id)}
							onchange={() => toggleAnnotation(item.id)}
						/>
						<span
							class="h-3 w-3 shrink-0 rounded-sm border border-(--annotation-color) bg-(--annotation-fill)"
							style:--annotation-color={item.color}
							style:--annotation-fill={item.fill}
						></span>
						<span class="truncate font-mono text-xs" title={item.label}>{item.label}</span>
					</label>
				{/each}
			</div>
		</div>
		<div class="relative max-w-full overflow-hidden rounded obs-preview">
			<img src={imageData} alt="OBS match source" class="block w-full" />
			<svg
				class="pointer-events-none absolute inset-0 h-full w-full"
				viewBox={`0 0 ${frameWidth} ${frameHeight}`}
				preserveAspectRatio="none"
				aria-hidden="true"
			>
				<defs>
					{#each placedAnnotations as item}
						<marker
							id={`${uid}-annotation-arrow-${item.index}`}
							viewBox="0 0 10 10"
							refX="9"
							refY="5"
							markerWidth="3"
							markerHeight="3"
							orient="auto-start-reverse"
						>
							<path d="M 0 0 L 10 5 L 0 10 z" fill={item.color} />
						</marker>
					{/each}
				</defs>

				{#each placedAnnotations as item}
					<rect
						x={item.region.x}
						y={item.region.y}
						width={item.region.w}
						height={item.region.h}
						fill={item.fill}
						stroke={item.color}
						stroke-width="3"
						vector-effect="non-scaling-stroke"
					/>
					<line
						x1={item.connectorStart.x}
						y1={item.connectorStart.y}
						x2={item.connectorEnd.x}
						y2={item.connectorEnd.y}
						stroke={item.color}
						stroke-width="2"
						vector-effect="non-scaling-stroke"
						marker-end={`url(#${uid}-annotation-arrow-${item.index})`}
					/>
				{/each}

				{#each placedAnnotations as item}
					<g>
						<rect
							x={item.label.x}
							y={item.label.y}
							width={item.label.w}
							height={item.label.h}
							rx="3"
							fill="rgba(0,0,0,0.82)"
							stroke={item.color}
							stroke-width="2"
							vector-effect="non-scaling-stroke"
						/>
						<text
							x={item.label.x + LABEL_PADDING_X}
							y={item.label.y + LABEL_PADDING_Y + 13}
							fill="white"
							font-family="ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace"
							font-size="14"
							font-weight="700"
						>
							{#each item.lines as line, lineIndex}
								<tspan
									x={item.label.x + LABEL_PADDING_X}
									dy={lineIndex === 0 ? 0 : LABEL_LINE_HEIGHT}
									fill={lineIndex === 0 ? item.color : 'white'}
								>
									{line}
								</tspan>
							{/each}
						</text>
					</g>
				{/each}
			</svg>
		</div>
	</div>
{/if}
