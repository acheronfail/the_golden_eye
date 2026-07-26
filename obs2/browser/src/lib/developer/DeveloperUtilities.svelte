<script lang="ts">
	import { backend, type AnnotationSet, type LevelMatch } from '$lib/api';
	import AnnotationOverlay from './AnnotationOverlay.svelte';
	import ScreenshotDatasetControls from './ScreenshotDatasetControls.svelte';
	import { triggerKiaDeathOverlay } from './effects';
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
	let screenshotLang = $state<'en' | 'jp'>('en');
	let annotationUpdateAbort: AbortController | null = null;
	let frameDumpUpdateAbort: AbortController | null = null;

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
			const res = await fetch(backend.apiUrl('/api/v1/sources'));
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
			const res = await fetch(backend.apiUrl(`/api/v1/screenshot?source=${encodeURIComponent(sourceName)}`));
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
			const result = await backend.matchSource(selectedSource.name, screenshotLang, { annotations: annotationMode });
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
			const result = await backend.matchUpload(file, screenshotLang, { annotations: true });
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
		void backend.setMonitorMatcherAnnotations(enabled, { signal: annotationUpdateAbort.signal }).catch((err) => {
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
		void backend.setMonitorFrameDump(enabled, source, { signal: frameDumpUpdateAbort.signal }).catch((err) => {
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
			void backend.setMonitorFrameDump(false, null, { keepalive: true }).catch(() => {});
		}
	});

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

	let annotationSets = $derived<AnnotationSet[]>(matchResult?.annotation_sets ?? []);
</script>

<div class="mx-auto flex w-full max-w-5xl flex-col gap-4 px-4 obs-page-top pb-4 sm:px-6">
	<h1 class="mb-4 text-2xl font-semibold obs-heading">Developer Utilities</h1>

	<div class="flex flex-col gap-3 rounded obs-panel px-4 py-3">
		<h2 class="text-xl font-semibold">Visual Effects</h2>
		<div class="flex flex-wrap gap-2">
			<button class="obs-button obs-button-danger px-3 py-1.5 text-sm" onclick={triggerKiaDeathOverlay}>
				trigger KIA overlay
			</button>
		</div>
	</div>

	<fieldset class="rounded obs-panel px-4 py-3" aria-labelledby="developer-language-heading">
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

	<fieldset class="rounded obs-panel px-4 py-3" aria-labelledby="developer-annotation-heading">
		<h2 id="developer-annotation-heading" class="mb-2 font-semibold">Annotation Mode</h2>
		<label class="flex items-center gap-2 pl-4">
			<input class="obs-checkbox" type="checkbox" bind:checked={annotationMode} />
			<span>Include matcher annotations</span>
		</label>
	</fieldset>

	<div class="flex flex-col gap-2 rounded obs-panel px-4 py-3">
		<h2 class="text-xl font-semibold">Match a frame from disk</h2>
		<p class="text-sm obs-muted">
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
			<span class="text-xs obs-dim">png / bmp — matched with {screenshotLang} templates</span>
		</button>
		<input
			bind:this={fileInput}
			type="file"
			accept="image/png,image/bmp,.png,.bmp"
			class="hidden"
			onchange={onPickFile}
		/>
	</div>

	<div class="flex flex-col gap-4 rounded obs-panel px-4 py-3">
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
					<p class="text-sm obs-muted">Selected source</p>
					<p class="font-mono text-lg">{selectedSource.name}</p>
					<p class="font-mono text-xs obs-dim">{selectedSource.id}</p>
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
					<p class="font-mono obs-dim">(not a video source)</p>
				{/if}
			</div>
		{:else if sources.length == 0}
			<p class="obs-dim">No sources, click "load sources" to fetch them from OBS.</p>
		{:else}
			<ul class="grid grid-cols-[max-content_1fr] items-center gap-x-4 gap-y-3">
				{#each sources as source}
					<li class="contents">
						<span class="text-right font-mono obs-muted">{source.name}: </span>

						<div class="flex flex-wrap gap-2">
							{#if knownVideoSourceIds.includes(source.id)}
								<button class="obs-button px-2 py-1 text-sm" onclick={() => selectSource(source)}>choose source</button>
							{:else}
								<span class="font-mono obs-dim">(not a video source)</span>
							{/if}
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	{#if screenshotError}
		<p class="rounded obs-alert-error px-4 py-3 font-mono text-sm">{screenshotError}</p>
	{/if}

	{#if matchError}
		<p class="rounded obs-alert-error px-4 py-3 font-mono text-sm">{matchError}</p>
	{/if}

	{#if matchResult}
		<div class="flex w-full flex-col gap-4 rounded obs-panel p-4">
			<div class="flex flex-row items-center gap-2">
				<h2 class="text-xl font-semibold">Level Match</h2>
				<button class="obs-button obs-button-danger px-2 py-1 font-mono text-sm" onclick={clearMatchResult}
					>close</button
				>
			</div>

			<div class="grid gap-4 lg:grid-cols-[minmax(18rem,24rem)_1fr]">
				{#if annotationsEnabled && imageData && annotationSets.length > 0 && matchFrameWidth > 0 && matchFrameHeight > 0}
					<AnnotationOverlay
						{imageData}
						{annotationSets}
						frameWidth={matchFrameWidth}
						frameHeight={matchFrameHeight}
						bind:selectedAnnotationSetId
						bind:hiddenAnnotationIds
					/>
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
		<ScreenshotDatasetControls
			{imageData}
			language={screenshotLang}
			streaming={Boolean(screenshottingSource)}
			close={clearImageData}
		/>
	{/if}
</div>
