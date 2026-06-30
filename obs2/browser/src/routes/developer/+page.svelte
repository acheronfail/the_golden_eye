<script lang="ts">
	import { apiUrl } from '$lib/api';
	import { settings } from '$lib/settings.svelte';
	import InputLang from '../../lib/InputLang.svelte';

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

	let allStartScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				values.push(`${settings.lang} - start - ${i} - ${d}`);
			}
		}

		return values;
	});

	let allFailedScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				for (const s of ['complete', 'failed', 'abort', 'kia']) {
					values.push(`${settings.lang} - ${s} - ${i} - ${d}`);
				}
			}
		}

		return values;
	});

	let statsScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				values.push(`${settings.lang} - stats - ${i} - ${d} - TIMES_HERE`);
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
		const res = await fetch(
			apiUrl(
				`/api/v1/screenshot?source=${encodeURIComponent(sourceName)}&lang=${encodeURIComponent(settings.lang)}`
			)
		);
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
</script>

<div class="flex flex-col gap-4 p-4">
	<h1 class="mb-4 text-2xl font-bold">Developer Utilities</h1>

	<InputLang />

	<div class="flex flex-col gap-4">
		<div class="flex flex-row gap-2">
			<h2 class="text-xl font-semibold">Available Sources:</h2>
			<button
				class="rounded bg-blue-500 px-2 py-1 font-semibold text-white hover:bg-blue-600 disabled:bg-slate-500 disabled:text-slate-300"
				disabled={sourcesLoading}
				onclick={getSources}>load sources</button
			>
		</div>

		{#if sources.length == 0}
			<p class="text-gray-500">No sources, click "load sources" to fetch them from OBS.</p>
		{:else}
			<ul class="grid grid-cols-[max-content_1fr] items-center gap-x-4 gap-y-3">
				{#each sources as source}
					<li class="contents">
						<span class="text-right font-mono">{source.name}: </span>

						<div class="flex flex-wrap gap-2">
							{#if knownVideoSourceIds.includes(source.id)}
								{#if !screenshottingSource}
									<button
										class="rounded bg-blue-500 px-2 py-1 text-white hover:bg-blue-600"
										onclick={getScreenshot(source.name)}>get screenshot</button
									>
								{/if}

								{#if screenshottingSource === source.name}
									<button
										class="rounded bg-red-500 px-2 py-1 text-white hover:bg-red-600"
										onclick={stopScreenshotting}>stop screenshotting</button
									>
								{:else}
									<button
										class="rounded bg-amber-500 px-2 py-1 text-white hover:bg-amber-600 disabled:bg-slate-500 disabled:text-slate-300"
										disabled={!!screenshottingSource}
										onclick={startScreenshotting(source.name)}>start screenshotting</button
									>
								{/if}
							{:else}
								<span class="font-mono text-gray-400">(not a video source)</span>
							{/if}
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	{#if imageData}
		<div class="flex w-1/2 flex-col gap-4 p-2">
			<div class="flex flex-row items-center gap-2">
				<h2 class="text-xl font-semibold">Screenshot:</h2>
				{#if !screenshottingSource}
					<button
						class="rounded bg-red-500 px-2 py-1 font-mono text-sm text-white hover:bg-red-600"
						onclick={() => (imageData = null)}
					>
						close
					</button>
				{/if}
			</div>

			<div class="flex flex-row gap-2">
				<button
					class="rounded bg-slate-500 px-2 py-1 font-mono text-sm text-white hover:bg-slate-600"
					onclick={() =>
						(startScreenIndex =
							(startScreenIndex - 1 + allStartScreenNames.length) % allStartScreenNames.length)}
					>-1</button
				>
				<button
					class="rounded bg-slate-500 px-2 py-1 font-mono text-sm text-white hover:bg-slate-600"
					onclick={() => (startScreenIndex = (startScreenIndex + 1) % allStartScreenNames.length)}
					>+1</button
				>
				<button
					class="rounded bg-blue-500 px-2 py-1 font-mono text-sm text-white hover:bg-blue-600"
					onclick={() =>
						(startScreenIndex = saveScreenshotAndAdvance(allStartScreenNames, startScreenIndex)())}
					>save "{allStartScreenNames[startScreenIndex]}.bmp"</button
				>
			</div>

			<div class="flex flex-row gap-2">
				<button
					class="rounded bg-slate-500 px-2 py-1 font-mono text-sm text-white hover:bg-slate-600"
					onclick={() =>
						(failedScreenIndex =
							(failedScreenIndex - 1 + allFailedScreenNames.length) % allFailedScreenNames.length)}
					>-1</button
				>
				<button
					class="rounded bg-slate-500 px-2 py-1 font-mono text-sm text-white hover:bg-slate-600"
					onclick={() =>
						(failedScreenIndex = (failedScreenIndex + 1) % allFailedScreenNames.length)}>+1</button
				>
				<button
					class="rounded bg-blue-500 px-2 py-1 font-mono text-sm text-white hover:bg-blue-600"
					onclick={() =>
						(failedScreenIndex = saveScreenshotAndAdvance(
							allFailedScreenNames,
							failedScreenIndex
						)())}>save "{allFailedScreenNames[failedScreenIndex]}.bmp"</button
				>
			</div>

			<div class="flex flex-row gap-2">
				<button
					class="rounded bg-slate-500 px-2 py-1 font-mono text-sm text-white hover:bg-slate-600"
					onclick={() =>
						(statsScreenIndex =
							(statsScreenIndex - 1 + statsScreenNames.length) % statsScreenNames.length)}
					>-1</button
				>
				<button
					class="rounded bg-slate-500 px-2 py-1 font-mono text-sm text-white hover:bg-slate-600"
					onclick={() => (statsScreenIndex = (statsScreenIndex + 1) % statsScreenNames.length)}
					>+1</button
				>
				<button
					class="rounded bg-blue-500 px-2 py-1 font-mono text-sm text-white hover:bg-blue-600"
					onclick={() =>
						(statsScreenIndex = saveScreenshotAndAdvance(statsScreenNames, statsScreenIndex)())}
					>save "{statsScreenNames[statsScreenIndex]}.bmp"</button
				>
			</div>

			<img src={imageData} alt="OBS Screenshot" class="max-w-full rounded" />
		</div>
	{/if}
</div>
