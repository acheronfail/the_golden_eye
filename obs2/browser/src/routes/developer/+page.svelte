<script lang="ts">
	import { apiUrl, matchSource, type LevelMatch } from '$lib/api';
	import { triggerKiaDeathOverlay } from '$lib/monitor.svelte';
	import { addNotificationFlag } from '$lib/notifications.svelte';

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
	let matchImageData = $state<string | null>(null);
	let diagnosticsEnabled = $state(false);
	let statsScreenIndex = $state(0);
	let startScreenIndex = $state(0);
	let failedScreenIndex = $state(0);
	let notificationTestCount = 0;
	let screenshotLang = $state<'en' | 'jp'>('en');

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
		diagnosticsEnabled = false;
		matchImageData = null;
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
			const result = await matchSource(selectedSource.name, screenshotLang);
			matchResult = result.match;
			diagnosticsEnabled = result.diagnosticsEnabled;
			matchImageData = `data:${result.imageMime};base64,${result.imageData}`;
		} catch (err) {
			matchError = err instanceof Error ? err.message : 'failed to match source';
		} finally {
			matchLoading = false;
		}
	};

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

						<button class="obs-button px-2 py-1 text-sm" disabled={matchLoading} onclick={runMatcher}>
							{matchLoading ? 'matching…' : 'match screenshot'}
						</button>
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
				{#if matchImageData}
					<img src={matchImageData} alt="Annotated OBS match" class="obs-preview max-w-full rounded" />
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
					<span class="obs-muted">diagnostics</span>
					<span class="font-mono">{diagnosticsEnabled ? 'enabled' : 'disabled'}</span>

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
