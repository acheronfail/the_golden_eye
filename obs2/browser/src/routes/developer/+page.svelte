<script lang="ts">
	import {
		apiUrl,
		matchSource,
		matchUpload,
		setMonitorFrameDump,
		setMonitorMatcherAnnotations,
		type AnnotationRect,
		type AnnotationSet,
		type LevelMatch
	} from '$lib/api';
	import { Select } from '$lib';
	import { triggerKiaDeathOverlay } from '$lib/monitor.svelte';
	import { addNotificationFlag } from '$lib/notifications.svelte';
	import { onDestroy } from 'svelte';

	const knownVideoSourceIds = [
		'screen_capture',
		'macos-avcapture',
		'macos-avcapture-fast',
		'ffmpeg_source',
		'v4l2_input'
	];

	let imageData = $state<string | null>(null);
	let sources = $state<{ name: string; id: string }[]>([]);
	let selectedSource = $state<{ name: string; id: string } | null>(null);
	let sourcesLoading = $state(false);
	let screenshottingSource = $state<string | null>(null);
	let screenshotError = $state<string | null>(null);
	let matchError = $state<string | null>(null);
	let matchLoading = $state(false);
	let matchResult = $state<LevelMatch | null>(null);
	let dragOver = $state(false);
	let fileInput = $state<HTMLInputElement | null>(null);
	let annotationMode = $state(false);
	let annotationsEnabled = $state(false);
	// Transient (not persisted), like annotation mode: dumps the selected source's
	// frames to disk (independent of the monitor) to compare live vs recorded input.
	let frameDumpMode = $state(false);
	let selectedAnnotationSetId = $state<string | null>(null);
	let hiddenAnnotationIds = $state<string[]>([]);
	let matchFrameWidth = $state(0);
	let matchFrameHeight = $state(0);
	let statsScreenIndex = $state(0);
	let startScreenIndex = $state(0);
	let failedScreenIndex = $state(0);
	let notificationTestCount = 0;
	let screenshotLang = $state<'en' | 'jp'>('en');
	let annotationUpdateAbort: AbortController | null = null;
	let frameDumpUpdateAbort: AbortController | null = null;

	let allStartScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				values.push(`${screenshotLang} - start - ${i} - ${d}`);
			}
		}

		return values;
	});

	let allFailedScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				for (const s of ['complete', 'failed', 'abort', 'kia']) {
					values.push(`${screenshotLang} - ${s} - ${i} - ${d}`);
				}
			}
		}

		return values;
	});

	let statsScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				values.push(`${screenshotLang} - stats - ${i} - ${d} - TIMES_HERE`);
			}
		}

		return values;
	});

	const saveScreenshotAndAdvance = (nameList: string[], index: number) => () => {
		if (!imageData) throw new Error('cannot screenshot without image data');

		const link = document.createElement('a');
		link.href = imageData;
		link.download = `${nameList[index]}.bmp`;
		link.click();

		return (index + 1) % nameList.length;
	};

	const clearImageData = () => {
		if (imageData) URL.revokeObjectURL(imageData);
		imageData = null;
	};

	const clearMatchResult = () => {
		matchResult = null;
		matchError = null;
		annotationsEnabled = false;
		selectedAnnotationSetId = null;
		hiddenAnnotationIds = [];
		matchFrameWidth = 0;
		matchFrameHeight = 0;
	};

	const getSources = async () => {
		sourcesLoading = true;
		try {
			const res = await fetch(apiUrl('/api/v1/sources'));
			const data = await res.json();
			sources = data;
		} finally {
			setTimeout(() => (sourcesLoading = false), 250);
		}
	};

	const selectSource = (source: { name: string; id: string }) => {
		selectedSource = source;
		clearMatchResult();
	};

	const closeSource = () => {
		stopScreenshotting();
		selectedSource = null;
		// Turn the dump off explicitly so it doesn't silently resume on a new source.
		frameDumpMode = false;
		clearImageData();
		clearMatchResult();
	};

	const getScreenshot = (sourceName: string) => async () => {
		screenshotError = null;
		try {
			const res = await fetch(apiUrl(`/api/v1/screenshot?source=${encodeURIComponent(sourceName)}`));
			if (!res.ok) throw new Error(`Request error: ${res.status} ${await res.text()}`);
			const blob = await res.blob();
			const url = URL.createObjectURL(blob);

			const old = imageData;
			imageData = url;
			if (old) URL.revokeObjectURL(old);
		} catch (err) {
			screenshotError = err instanceof Error ? err.message : 'failed to capture screenshot';
		}
	};

	const stopScreenshotting = () => {
		screenshottingSource = null;
	};
	const startScreenshotting = (sourceName: string) => async () => {
		screenshottingSource = sourceName;
		while (screenshottingSource) {
			await getScreenshot(screenshottingSource)();
			await new Promise((resolve) => setTimeout(resolve, 10));
		}
	};

	const runMatcher = async () => {
		if (!selectedSource) return;

		matchLoading = true;
		matchError = null;
		try {
			if (annotationMode) {
				await getScreenshot(selectedSource.name)();
			}
			const result = await matchSource(selectedSource.name, screenshotLang, { annotations: annotationMode });
			matchResult = result.match;
			annotationsEnabled = result.annotationsEnabled;
			matchFrameWidth = result.frameWidth;
			matchFrameHeight = result.frameHeight;
			selectedAnnotationSetId = result.match.annotation_sets?.[0]?.id ?? null;
			hiddenAnnotationIds = [];
		} catch (err) {
			matchError = err instanceof Error ? err.message : 'failed to match source';
		} finally {
			matchLoading = false;
		}
	};

	// Match a frame dropped in / picked from disk (e.g. a dumped bmp), always with
	// annotations so the digit slot diagnostics render.
	const matchFile = async (file: File) => {
		matchLoading = true;
		matchError = null;
		try {
			const result = await matchUpload(file, screenshotLang, { annotations: true });
			// Show the dropped image itself under the annotation overlay.
			const old = imageData;
			imageData = URL.createObjectURL(file);
			if (old) URL.revokeObjectURL(old);
			matchResult = result.match;
			annotationsEnabled = result.annotationsEnabled;
			matchFrameWidth = result.frameWidth;
			matchFrameHeight = result.frameHeight;
			// Default to the digit-slot diagnostics — the reason to drop a frame here.
			const sets = result.match.annotation_sets ?? [];
			selectedAnnotationSetId = sets.find((set) => set.id === 'time_digits')?.id ?? sets[0]?.id ?? null;
			hiddenAnnotationIds = [];
		} catch (err) {
			matchError = err instanceof Error ? err.message : 'failed to match uploaded image';
		} finally {
			matchLoading = false;
		}
	};

	const onDropFiles = (event: DragEvent) => {
		event.preventDefault();
		dragOver = false;
		const file = event.dataTransfer?.files?.[0];
		if (file) void matchFile(file);
	};

	const onPickFile = (event: Event) => {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		if (file) void matchFile(file);
		// Reset so re-selecting the same file fires change again.
		input.value = '';
	};

	const updateMonitorAnnotations = (enabled: boolean) => {
		annotationUpdateAbort?.abort();
		annotationUpdateAbort = new AbortController();
		void setMonitorMatcherAnnotations(enabled, { signal: annotationUpdateAbort.signal }).catch((err) => {
			if (err instanceof DOMException && err.name === 'AbortError') return;
			console.warn('Failed to update monitor annotation diagnostics', err);
		});
	};

	$effect(() => {
		updateMonitorAnnotations(annotationMode);
	});

	const updateFrameDump = (enabled: boolean, source: string | null) => {
		frameDumpUpdateAbort?.abort();
		frameDumpUpdateAbort = new AbortController();
		void setMonitorFrameDump(enabled, source, { signal: frameDumpUpdateAbort.signal }).catch((err) => {
			if (err instanceof DOMException && err.name === 'AbortError') return;
			console.warn('Failed to update monitor frame dump', err);
		});
	};

	// The dump needs a source; enabling without one is disabled in the UI, and
	// clearing the source (or closing it) turns the dump off. Re-runs on either
	// change, restarting the dump against the new source.
	$effect(() => {
		const source = selectedSource?.name ?? null;
		updateFrameDump(frameDumpMode && source !== null, source);
	});

	// Stop the transient frame dump when leaving the page. `keepalive` lets the
	// request outlive the unloading document.
	onDestroy(() => {
		if (frameDumpMode) {
			frameDumpUpdateAbort?.abort();
			void setMonitorFrameDump(false, null, { keepalive: true }).catch(() => {});
		}
	});

	const addTestNotification = () => {
		notificationTestCount += 1;
		addNotificationFlag({
			title: `Test notification ${notificationTestCount}`,
			detail: 'This notification was triggered from Developer Utilities.',
			meta: new Date().toLocaleTimeString(),
			tone: 'info'
		});
	};

	const formatSeconds = (value: number | null | undefined) => {
		if (value == null || value < 0) return 'none';
		const minutes = Math.floor(value / 60);
		const seconds = value % 60;
		return `${minutes}:${seconds.toString().padStart(2, '0')}`;
	};

	const screenLabel = (value: string) =>
		value
			.replace(/_/g, ' ')
			.replace(/([a-z])([A-Z])/g, '$1 $2')
			.toLowerCase();

	const annotationColors = [
		'#22d3ee',
		'#fbbf24',
		'#fb7185',
		'#a78bfa',
		'#34d399',
		'#f97316',
		'#60a5fa',
		'#f472b6',
		'#bef264',
		'#2dd4bf'
	];
	const labelPaddingX = 8;
	const labelPaddingY = 5;
	const labelLineHeight = 18;
	const labelCharWidth = 8.3;
	const labelMaxChars = 28;
	const labelMargin = 6;

	interface OverlayBox {
		x: number;
		y: number;
		w: number;
		h: number;
	}

	interface OverlayPoint {
		x: number;
		y: number;
	}

	interface PlacedAnnotation {
		index: number;
		id: string;
		region: OverlayBox;
		label: OverlayBox;
		lines: string[];
		color: string;
		fill: string;
		connectorStart: OverlayPoint;
		connectorEnd: OverlayPoint;
	}

	interface AnnotationListItem {
		id: string;
		index: number;
		annotation: AnnotationRect;
		label: string;
		color: string;
		fill: string;
	}

	let annotationSets = $derived<AnnotationSet[]>(matchResult?.annotation_sets ?? []);
	let annotationSetOptions = $derived(annotationSets.map((set) => ({ value: set.id, label: set.label })));
	let selectedAnnotationSet = $derived<AnnotationSet | null>(
		annotationSets.find((set) => set.id === selectedAnnotationSetId) ?? annotationSets[0] ?? null
	);

	const annotationText = (annotation: AnnotationRect) =>
		annotation.score == null ? annotation.label : `${annotation.label} ${annotation.score.toFixed(2)}`;

	const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max);

	const annotationColor = (index: number) => annotationColors[index % annotationColors.length];

	const annotationFill = (color: string) => `${color}26`;

	const annotationId = (set: AnnotationSet, index: number) => `${set.id}:${index}`;

	const annotationListItems = (set: AnnotationSet | null): AnnotationListItem[] =>
		set?.annotations.map((annotation, index) => {
			const color = annotationColor(index);
			return {
				id: annotationId(set, index),
				index,
				annotation,
				label: annotationText(annotation),
				color,
				fill: annotationFill(color)
			};
		}) ?? [];

	const toggleAnnotation = (id: string) => {
		hiddenAnnotationIds = hiddenAnnotationIds.includes(id)
			? hiddenAnnotationIds.filter((item) => item !== id)
			: [...hiddenAnnotationIds, id];
	};

	const normalizeRegion = (annotation: AnnotationRect): OverlayBox => {
		const frameW = Math.max(1, matchFrameWidth);
		const frameH = Math.max(1, matchFrameHeight);
		const x = clamp(annotation.x, 0, frameW - 1);
		const y = clamp(annotation.y, 0, frameH - 1);
		return {
			x,
			y,
			w: clamp(annotation.w, 1, frameW - x),
			h: clamp(annotation.h, 1, frameH - y)
		};
	};

	const wrapLabel = (text: string): string[] => {
		const words = text.split(/\s+/).filter(Boolean);
		const lines: string[] = [];
		let line = '';

		for (const word of words) {
			const next = line ? `${line} ${word}` : word;
			if (next.length <= labelMaxChars) {
				line = next;
				continue;
			}
			if (line) lines.push(line);
			if (word.length <= labelMaxChars) {
				line = word;
			} else {
				lines.push(word.slice(0, labelMaxChars - 1));
				line = word.slice(labelMaxChars - 1);
			}
		}
		if (line) lines.push(line);
		return lines.length ? lines : [text];
	};

	const labelSize = (lines: string[]): { w: number; h: number } => ({
		w: Math.max(56, Math.ceil(Math.max(...lines.map((line) => line.length)) * labelCharWidth + labelPaddingX * 2)),
		h: Math.ceil(lines.length * labelLineHeight + labelPaddingY * 2)
	});

	const clampLabel = (box: OverlayBox, frameW: number, frameH: number): OverlayBox => ({
		...box,
		x: clamp(box.x, labelMargin, Math.max(labelMargin, frameW - box.w - labelMargin)),
		y: clamp(box.y, labelMargin, Math.max(labelMargin, frameH - box.h - labelMargin))
	});

	const boxCenter = (box: OverlayBox): OverlayPoint => ({
		x: box.x + box.w / 2,
		y: box.y + box.h / 2
	});

	const overlapArea = (a: OverlayBox, b: OverlayBox): number => {
		const x = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
		const y = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
		return x * y;
	};

	const distanceSquared = (a: OverlayPoint, b: OverlayPoint): number => {
		const dx = a.x - b.x;
		const dy = a.y - b.y;
		return dx * dx + dy * dy;
	};

	const connectorStart = (label: OverlayBox, target: OverlayPoint): OverlayPoint => ({
		x: clamp(target.x, label.x, label.x + label.w),
		y: clamp(target.y, label.y, label.y + label.h)
	});

	const labelCandidates = (region: OverlayBox, label: { w: number; h: number }, frameW: number, frameH: number) => {
		const center = boxCenter(region);
		const candidates: OverlayBox[] = [];
		const offsets = [10, 34, 66, 104, 146];

		for (const offset of offsets) {
			candidates.push({ x: center.x - label.w / 2, y: region.y - label.h - offset, w: label.w, h: label.h });
			candidates.push({ x: center.x - label.w / 2, y: region.y + region.h + offset, w: label.w, h: label.h });
			candidates.push({ x: region.x + region.w + offset, y: center.y - label.h / 2, w: label.w, h: label.h });
			candidates.push({ x: region.x - label.w - offset, y: center.y - label.h / 2, w: label.w, h: label.h });
			candidates.push({ x: region.x + region.w + offset, y: region.y - label.h - offset, w: label.w, h: label.h });
			candidates.push({ x: region.x - label.w - offset, y: region.y - label.h - offset, w: label.w, h: label.h });
			candidates.push({ x: region.x + region.w + offset, y: region.y + region.h + offset, w: label.w, h: label.h });
			candidates.push({ x: region.x - label.w - offset, y: region.y + region.h + offset, w: label.w, h: label.h });
		}

		const laneYStep = label.h + 5;
		const laneXs = [labelMargin, frameW - label.w - labelMargin, frameW / 2 - label.w / 2];
		for (const x of laneXs) {
			for (let y = labelMargin; y <= frameH - label.h - labelMargin; y += laneYStep) {
				candidates.push({ x, y, w: label.w, h: label.h });
			}
		}

		return candidates.map((candidate) => clampLabel(candidate, frameW, frameH));
	};

	const placeAnnotations = (items: AnnotationListItem[]): PlacedAnnotation[] => {
		if (matchFrameWidth <= 0 || matchFrameHeight <= 0) return [];

		const frameW = matchFrameWidth;
		const frameH = matchFrameHeight;
		const regions = items.map((item) => normalizeRegion(item.annotation));
		const occupied: OverlayBox[] = [];

		return items.map((item, itemIndex) => {
			const { index, id, label: labelText, color, fill } = item;
			const region = regions[itemIndex];
			const lines = wrapLabel(labelText);
			const size = labelSize(lines);
			const target = boxCenter(region);
			const label = labelCandidates(region, size, frameW, frameH)
				.map((candidate) => {
					const labelOverlap = occupied.reduce((sum, box) => sum + overlapArea(candidate, box), 0);
					const regionOverlap = regions.reduce((sum, box) => sum + overlapArea(candidate, box), 0);
					return {
						box: candidate,
						score:
							labelOverlap * 100_000 +
							overlapArea(candidate, region) * 1_000 +
							regionOverlap * 25 +
							distanceSquared(boxCenter(candidate), target)
					};
				})
				.sort((a, b) => a.score - b.score)[0].box;

			occupied.push(label);
			return {
				index,
				id,
				region,
				label,
				lines,
				color,
				fill: annotationFill(color),
				connectorStart: connectorStart(label, target),
				connectorEnd: target
			};
		});
	};

	let annotationItems = $derived<AnnotationListItem[]>(annotationListItems(selectedAnnotationSet));
	let visibleAnnotationItems = $derived<AnnotationListItem[]>(
		annotationItems.filter((item) => !hiddenAnnotationIds.includes(item.id))
	);
	let placedAnnotations = $derived<PlacedAnnotation[]>(placeAnnotations(visibleAnnotationItems));
</script>

<div class="mx-auto flex w-full max-w-5xl flex-col gap-4 p-4">
	<h1 class="obs-heading mb-4 text-2xl font-bold">Developer Utilities</h1>

	<div class="obs-panel flex flex-col gap-3 rounded px-4 py-3">
		<h2 class="text-xl font-semibold">Visual Effects</h2>
		<div class="flex flex-wrap gap-2">
			<button class="obs-button obs-button-danger px-3 py-1.5 text-sm" onclick={triggerKiaDeathOverlay}>
				trigger KIA overlay
			</button>
			<button class="obs-button obs-button-gold px-3 py-1.5 text-sm" onclick={addTestNotification}>
				add test notification
			</button>
		</div>
	</div>

	<fieldset class="obs-panel rounded px-4 py-3" aria-labelledby="developer-language-heading">
		<h2 id="developer-language-heading" class="mb-2 font-semibold">Language</h2>
		<div class="flex flex-col gap-1 pl-4">
			<label class="flex items-center gap-2">
				<input class="obs-checkbox" type="radio" name="lang" value="en" bind:group={screenshotLang} />
				English
			</label>
			<label class="flex items-center gap-2">
				<input class="obs-checkbox" type="radio" name="lang" value="jp" bind:group={screenshotLang} />
				Japanese
			</label>
		</div>
	</fieldset>

	<fieldset class="obs-panel rounded px-4 py-3" aria-labelledby="developer-annotation-heading">
		<h2 id="developer-annotation-heading" class="mb-2 font-semibold">Annotation Mode</h2>
		<label class="flex items-center gap-2 pl-4">
			<input class="obs-checkbox" type="checkbox" bind:checked={annotationMode} />
			<span>Include matcher annotations</span>
		</label>
	</fieldset>

	<div class="obs-panel flex flex-col gap-2 rounded px-4 py-3">
		<h2 class="text-xl font-semibold">Match a frame from disk</h2>
		<p class="obs-muted text-sm">
			Drop or select a dumped frame (png/bmp) to match it with annotations. The <code>Time digits</code>
			set shows where each digit was read from — a detection box offset from its colon-anchored slot is a misalignment.
		</p>
		<button
			type="button"
			class="obs-preview-missing flex min-h-28 w-full flex-col items-center justify-center gap-1 rounded px-4 py-6 text-sm transition-colors {dragOver
				? 'border-white/70 bg-white/5 text-white'
				: ''}"
			class:opacity-70={matchLoading}
			ondragover={(e) => {
				e.preventDefault();
				dragOver = true;
			}}
			ondragleave={() => (dragOver = false)}
			ondrop={onDropFiles}
			onclick={() => fileInput?.click()}
		>
			<span class="font-semibold">{matchLoading ? 'matching…' : 'Click to select, or drop an image here'}</span>
			<span class="obs-dim text-xs">png / bmp — matched with {screenshotLang} templates</span>
		</button>
		<input
			bind:this={fileInput}
			type="file"
			accept="image/png,image/bmp,.png,.bmp"
			class="hidden"
			onchange={onPickFile}
		/>
	</div>

	<div class="obs-panel flex flex-col gap-4 rounded px-4 py-3">
		<div class="flex flex-row gap-2">
			<h2 class="text-xl font-semibold">Source</h2>
			<button class="obs-button obs-button-gold px-2 py-1 text-sm" disabled={sourcesLoading} onclick={getSources}
				>load sources</button
			>
			{#if selectedSource}
				<button class="obs-button obs-button-danger px-2 py-1 text-sm" onclick={closeSource}>close source</button>
			{/if}
		</div>

		{#if selectedSource}
			<div class="flex flex-col gap-3">
				<div>
					<p class="obs-muted text-sm">Selected source</p>
					<p class="font-mono text-lg">{selectedSource.name}</p>
					<p class="obs-dim font-mono text-xs">{selectedSource.id}</p>
				</div>

				{#if knownVideoSourceIds.includes(selectedSource.id)}
					<div class="flex flex-wrap gap-2">
						{#if !screenshottingSource}
							<button class="obs-button px-2 py-1 text-sm" onclick={getScreenshot(selectedSource.name)}
								>get screenshot</button
							>
							<button class="obs-button px-2 py-1 text-sm" disabled={matchLoading} onclick={runMatcher}>
								{matchLoading ? 'matching…' : 'match screenshot'}
							</button>
						{/if}

						{#if screenshottingSource === selectedSource.name}
							<button class="obs-button obs-button-danger px-2 py-1 text-sm" onclick={stopScreenshotting}
								>stop screenshotting</button
							>
						{:else}
							<button
								class="obs-button obs-button-gold px-2 py-1 text-sm"
								disabled={!!screenshottingSource}
								onclick={startScreenshotting(selectedSource.name)}>start screenshotting</button
							>
						{/if}

						{#if frameDumpMode}
							<button class="obs-button obs-button-danger px-2 py-1 text-sm" onclick={() => (frameDumpMode = false)}
								>stop frame dump</button
							>
						{:else}
							<button
								class="obs-button px-2 py-1 text-sm"
								title="Dump this source's frames to a temp folder (path logged to the OBS log), independent of the monitor. Stops on reload or when the source is closed."
								onclick={() => (frameDumpMode = true)}>start frame dump</button
							>
						{/if}
					</div>
				{:else}
					<p class="obs-dim font-mono">(not a video source)</p>
				{/if}
			</div>
		{:else if sources.length == 0}
			<p class="obs-dim">No sources, click "load sources" to fetch them from OBS.</p>
		{:else}
			<ul class="grid grid-cols-[max-content_1fr] items-center gap-x-4 gap-y-3">
				{#each sources as source}
					<li class="contents">
						<span class="obs-muted text-right font-mono">{source.name}: </span>

						<div class="flex flex-wrap gap-2">
							{#if knownVideoSourceIds.includes(source.id)}
								<button class="obs-button px-2 py-1 text-sm" onclick={() => selectSource(source)}>choose source</button>
							{:else}
								<span class="obs-dim font-mono">(not a video source)</span>
							{/if}
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	{#if screenshotError}
		<p class="obs-alert-error rounded px-4 py-3 font-mono text-sm">{screenshotError}</p>
	{/if}

	{#if matchError}
		<p class="obs-alert-error rounded px-4 py-3 font-mono text-sm">{matchError}</p>
	{/if}

	{#if matchResult}
		<div class="obs-panel flex w-full flex-col gap-4 rounded p-4">
			<div class="flex flex-row items-center gap-2">
				<h2 class="text-xl font-semibold">Level Match</h2>
				<button class="obs-button obs-button-danger px-2 py-1 font-mono text-sm" onclick={clearMatchResult}
					>close</button
				>
			</div>

			<div class="grid gap-4 lg:grid-cols-[minmax(18rem,24rem)_1fr]">
				{#if annotationsEnabled && imageData && selectedAnnotationSet && matchFrameWidth > 0 && matchFrameHeight > 0}
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
								<span class="obs-dim font-mono text-xs">{visibleAnnotationItems.length}/{annotationItems.length}</span>
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
											class="h-3 w-3 shrink-0 rounded-sm border"
											style={`border-color:${item.color};background:${item.fill}`}
										></span>
										<span class="truncate font-mono text-xs" title={item.label}>{item.label}</span>
									</label>
								{/each}
							</div>
						</div>
						<div class="obs-preview relative max-w-full overflow-hidden rounded">
							<img src={imageData} alt="OBS match source" class="block w-full" />
							<svg
								class="pointer-events-none absolute inset-0 h-full w-full"
								viewBox={`0 0 ${matchFrameWidth} ${matchFrameHeight}`}
								preserveAspectRatio="none"
								aria-hidden="true"
							>
								<defs>
									{#each placedAnnotations as item}
										<marker
											id={`annotation-arrow-${item.index}`}
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
										marker-end={`url(#annotation-arrow-${item.index})`}
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
											x={item.label.x + labelPaddingX}
											y={item.label.y + labelPaddingY + 13}
											fill="white"
											font-family="ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace"
											font-size="14"
											font-weight="700"
										>
											{#each item.lines as line, lineIndex}
												<tspan
													x={item.label.x + labelPaddingX}
													dy={lineIndex === 0 ? 0 : labelLineHeight}
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

				<div class="grid grid-cols-[max-content_1fr] gap-x-4 gap-y-2 text-sm">
					<span class="obs-muted">screen</span>
					<span class="font-mono">{screenLabel(matchResult.screen)}</span>
					<span class="obs-muted">mission</span>
					<span class="font-mono">{matchResult.mission}</span>
					<span class="obs-muted">part</span>
					<span class="font-mono">{matchResult.part}</span>
					<span class="obs-muted">difficulty</span>
					<span class="font-mono">{matchResult.difficulty}</span>
					<span class="obs-muted">detected lang</span>
					<span class="font-mono">{matchResult.detected_lang ?? 'none'}</span>
					<span class="obs-muted">runtime</span>
					<span class="font-mono">{matchResult.runtime_ms.toFixed(2)} ms</span>
					<span class="obs-muted">regions</span>
					<span class="font-mono">{matchResult.match_regions?.length ?? 0}</span>
					<span class="obs-muted">annotations</span>
					{#if annotationsEnabled}
						<span class="font-mono">enabled</span>
					{:else}
						<span class="font-mono">disabled</span>
					{/if}

					{#if matchResult.times}
						<span class="obs-muted">time</span>
						<span class="font-mono">{formatSeconds(matchResult.times.time)}</span>
						<span class="obs-muted">target</span>
						<span class="font-mono">{formatSeconds(matchResult.times.target_time)}</span>
						<span class="obs-muted">best</span>
						<span class="font-mono">{formatSeconds(matchResult.times.best_time)}</span>
					{/if}

					{#if matchResult.raw_times?.length}
						<span class="obs-muted">raw times</span>
						<span class="font-mono">{matchResult.raw_times.map(formatSeconds).join(', ')}</span>
					{/if}
				</div>
			</div>
		</div>
	{/if}

	{#if imageData}
		<div class="obs-panel flex w-full flex-col gap-4 rounded p-4">
			<div class="flex flex-row items-center gap-2">
				<h2 class="text-xl font-semibold">Screenshot:</h2>
				{#if !screenshottingSource}
					<button class="obs-button obs-button-danger px-2 py-1 font-mono text-sm" onclick={clearImageData}>
						close
					</button>
				{/if}
			</div>

			<div class="flex flex-row gap-2">
				<button
					class="obs-button px-2 py-1 font-mono text-sm"
					onclick={() =>
						(startScreenIndex = (startScreenIndex - 1 + allStartScreenNames.length) % allStartScreenNames.length)}
					>-1</button
				>
				<button
					class="obs-button px-2 py-1 font-mono text-sm"
					onclick={() => (startScreenIndex = (startScreenIndex + 1) % allStartScreenNames.length)}>+1</button
				>
				<button
					class="obs-button obs-button-gold px-2 py-1 font-mono text-sm"
					onclick={() => (startScreenIndex = saveScreenshotAndAdvance(allStartScreenNames, startScreenIndex)())}
					>save "{allStartScreenNames[startScreenIndex]}.bmp"</button
				>
			</div>

			<div class="flex flex-row gap-2">
				<button
					class="obs-button px-2 py-1 font-mono text-sm"
					onclick={() =>
						(failedScreenIndex = (failedScreenIndex - 1 + allFailedScreenNames.length) % allFailedScreenNames.length)}
					>-1</button
				>
				<button
					class="obs-button px-2 py-1 font-mono text-sm"
					onclick={() => (failedScreenIndex = (failedScreenIndex + 1) % allFailedScreenNames.length)}>+1</button
				>
				<button
					class="obs-button obs-button-gold px-2 py-1 font-mono text-sm"
					onclick={() => (failedScreenIndex = saveScreenshotAndAdvance(allFailedScreenNames, failedScreenIndex)())}
					>save "{allFailedScreenNames[failedScreenIndex]}.bmp"</button
				>
			</div>

			<div class="flex flex-row gap-2">
				<button
					class="obs-button px-2 py-1 font-mono text-sm"
					onclick={() =>
						(statsScreenIndex = (statsScreenIndex - 1 + statsScreenNames.length) % statsScreenNames.length)}>-1</button
				>
				<button
					class="obs-button px-2 py-1 font-mono text-sm"
					onclick={() => (statsScreenIndex = (statsScreenIndex + 1) % statsScreenNames.length)}>+1</button
				>
				<button
					class="obs-button obs-button-gold px-2 py-1 font-mono text-sm"
					onclick={() => (statsScreenIndex = saveScreenshotAndAdvance(statsScreenNames, statsScreenIndex)())}
					>save "{statsScreenNames[statsScreenIndex]}.bmp"</button
				>
			</div>

			<img src={imageData} alt="OBS Screenshot" class="obs-preview max-w-full rounded" />
		</div>
	{/if}
</div>
