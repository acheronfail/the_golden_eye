<script lang="ts">
	import { apiUrl } from '$lib/api';
	import { triggerKiaDeathOverlay } from '$lib/monitor.svelte';
	import { addNotificationFlag } from '$lib/notifications.svelte';
	import { settings } from '$lib/settings.svelte';

	const knownVideoSourceIds = [
		'screen_capture',
		'macos-avcapture',
		'macos-avcapture-fast',
		'ffmpeg_source',
		'v4l2_input'
	];

	let imageData = $state<string | null>(null);
	let sources = $state<{ name: string; id: string }[]>([]);
	let sourcesLoading = $state(false);
	let screenshottingSource = $state<string | null>(null);
	let statsScreenIndex = $state(0);
	let startScreenIndex = $state(0);
	let failedScreenIndex = $state(0);
	let notificationTestCount = 0;

	let allStartScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				values.push(`${settings.developerLang} - start - ${i} - ${d}`);
			}
		}

		return values;
	});

	let allFailedScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				for (const s of ['complete', 'failed', 'abort', 'kia']) {
					values.push(`${settings.developerLang} - ${s} - ${i} - ${d}`);
				}
			}
		}

		return values;
	});

	let statsScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				values.push(`${settings.developerLang} - stats - ${i} - ${d} - TIMES_HERE`);
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

	const getSources = async () => {
		sourcesLoading = true;
		const res = await fetch(apiUrl('/api/v1/sources'));
		const data = await res.json();
		sources = data;
		setTimeout(() => (sourcesLoading = false), 250);
	};

	const getScreenshot = (sourceName: string) => async () => {
		const res = await fetch(apiUrl(`/api/v1/screenshot?source=${encodeURIComponent(sourceName)}`));
		const blob = await res.blob();
		const url = URL.createObjectURL(blob);

		const old = imageData;
		imageData = url;
		if (old) URL.revokeObjectURL(old);
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

	const addTestNotification = () => {
		notificationTestCount += 1;
		addNotificationFlag({
			title: `Test notification ${notificationTestCount}`,
			detail: 'This notification was triggered from Developer Utilities.',
			meta: new Date().toLocaleTimeString(),
			tone: 'info'
		});
	};
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
				<input class="obs-checkbox" type="radio" name="lang" value="en" bind:group={settings.developerLang} />
				English
			</label>
			<label class="flex items-center gap-2">
				<input class="obs-checkbox" type="radio" name="lang" value="jp" bind:group={settings.developerLang} />
				Japanese
			</label>
		</div>
	</fieldset>

	<div class="obs-panel flex flex-col gap-4 rounded px-4 py-3">
		<div class="flex flex-row gap-2">
			<h2 class="text-xl font-semibold">Available Sources:</h2>
			<button class="obs-button obs-button-gold px-2 py-1 text-sm" disabled={sourcesLoading} onclick={getSources}
				>load sources</button
			>
		</div>

		{#if sources.length == 0}
			<p class="obs-dim">No sources, click "load sources" to fetch them from OBS.</p>
		{:else}
			<ul class="grid grid-cols-[max-content_1fr] items-center gap-x-4 gap-y-3">
				{#each sources as source}
					<li class="contents">
						<span class="obs-muted text-right font-mono">{source.name}: </span>

						<div class="flex flex-wrap gap-2">
							{#if knownVideoSourceIds.includes(source.id)}
								{#if !screenshottingSource}
									<button class="obs-button px-2 py-1 text-sm" onclick={getScreenshot(source.name)}
										>get screenshot</button
									>
								{/if}

								{#if screenshottingSource === source.name}
									<button class="obs-button obs-button-danger px-2 py-1 text-sm" onclick={stopScreenshotting}
										>stop screenshotting</button
									>
								{:else}
									<button
										class="obs-button obs-button-gold px-2 py-1 text-sm"
										disabled={!!screenshottingSource}
										onclick={startScreenshotting(source.name)}>start screenshotting</button
									>
								{/if}
							{:else}
								<span class="obs-dim font-mono">(not a video source)</span>
							{/if}
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	{#if imageData}
		<div class="obs-panel flex w-full flex-col gap-4 rounded p-4">
			<div class="flex flex-row items-center gap-2">
				<h2 class="text-xl font-semibold">Screenshot:</h2>
				{#if !screenshottingSource}
					<button class="obs-button obs-button-danger px-2 py-1 font-mono text-sm" onclick={() => (imageData = null)}>
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
