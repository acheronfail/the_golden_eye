<script lang="ts">
	import { apiUrl } from '$lib/api';

	let imageData = $state<string | null>(null);
	let sources = $state<{ name: string; id: string }[]>([]);
	let monitoring = $state(false);
	let lang = $state<'en' | 'jp'>('jp');
	let lastSourceUsed = $state<string | null>(null);
	let statsScreenIndex = $state(0);
	let startScreenIndex = $state(0);
	let failedScreenIndex = $state(0);

	let allStartScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				values.push(`${lang} - start - ${i} - ${d}`);
			}
		}

		return values;
	});

	let allFailedScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				for (const s of ['complete', 'failed', 'abort', 'kia']) {
					values.push(`${lang} - ${s} - ${i} - ${d}`);
				}
			}
		}

		return values;
	});

	let statsScreenNames = $derived.by(() => {
		const values: string[] = [];
		for (let i = 1; i <= 20; i++) {
			for (const d of ['Agent', 'Secret Agent', '00 Agent']) {
				values.push(`${lang} - stats - ${i} - ${d} - TIMES_HERE`);
			}
		}

		return values;
	});

	let interval: number | null = null;
	$effect(() => {
		if (!lastSourceUsed) return;

		interval = setInterval(() => {
			lastSourceUsed && getScreenshot(lastSourceUsed)();
		}, 150);
		return () => interval && clearInterval(interval);
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
		const res = await fetch(apiUrl('/api/v1/sources'));
		const data = await res.json();
		sources = data;
	};

	const getScreenshot = (sourceName: string) => async () => {
		const res = await fetch(apiUrl(`/api/v1/screenshot?source=${encodeURIComponent(sourceName)}`));
		const blob = await res.blob();
		const url = URL.createObjectURL(blob);
		imageData = url;

		lastSourceUsed = sourceName;
	};

	const startMonitor = (sourceName: string) => async () => {
		const res = await fetch(apiUrl(`/api/v1/monitor/start`), {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ sourceName })
		});
		if (res.ok) {
			monitoring = true;
		} else {
			alert(`Request error: ${res.status} ${await res.text()}`);
		}
	};
	const stopMonitor = async () => {
		const res = await fetch(apiUrl(`/api/v1/monitor/stop`), {
			method: 'POST',
			headers: { 'content-type': 'application/json' }
		});
		if (res.ok) {
			monitoring = false;
		} else {
			alert(`Request error: ${res.status} ${await res.text()}`);
		}
	};
</script>

<!-- TODO: screen to set ROIs for goldeneye -->

<div>
	<h1 class="mb-4 text-2xl font-bold">Welcome to Goldeneye!</h1>
	<p class="mb-4">This is the main dashboard.</p>

	<fieldset class="mb-4">
		<legend class="mb-2 font-semibold">Language:</legend>
		<label class="mr-4">
			<input type="radio" name="lang" value="en" bind:group={lang} />
			English
		</label>
		<label>
			<input type="radio" name="lang" value="jp" bind:group={lang} />
			Japanese
		</label>
	</fieldset>

	<button
		class="mb-4 rounded bg-blue-500 px-4 py-2 font-semibold text-white hover:bg-blue-600"
		onclick={getSources}>get sources</button
	>

	{#if sources.length > 0}
		<div class="mb-4">
			<h2 class="mb-2 text-xl font-semibold">Available Sources:</h2>
			<ul class="list-inside list-disc">
				{#each sources as source}
					<li class="flex gap-4">
						{source.name}
						{#if ['screen_capture', 'macos-avcapture', 'macos-avcapture-fast', 'v4l2_input'].includes(source.id)}
							<button
								class="ml-2 rounded bg-blue-500 px-2 py-1 text-white hover:bg-blue-600"
								onclick={getScreenshot(source.name)}>get screenshot</button
							>
							{#if !monitoring}
								<button
									class="ml-2 rounded bg-green-500 px-2 py-1 text-white hover:bg-green-600"
									onclick={startMonitor(source.name)}>start monitor</button
								>
							{:else}
								<button
									class="ml-2 rounded bg-red-500 px-2 py-1 text-white hover:bg-red-600"
									onclick={stopMonitor}>stop monitor</button
								>
							{/if}
						{/if}
					</li>
				{/each}
			</ul>
		</div>
	{:else}
		<p class="mb-4 text-gray-500">No sources, click "get sources" to fetch them from OBS.</p>
	{/if}

	{#if imageData}
		<div class="flex w-1/2 flex-col gap-4 p-2">
			<h2 class="text-xl font-semibold">Screenshot:</h2>
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
					onclick={() =>
						(statsScreenIndex = (statsScreenIndex + 1) % statsScreenNames.length)}>+1</button
				>
				<button
					class="rounded bg-blue-500 px-2 py-1 font-mono text-sm text-white hover:bg-blue-600"
					onclick={() =>
						(statsScreenIndex = saveScreenshotAndAdvance(
							statsScreenNames,
							statsScreenIndex
						)())}>save "{statsScreenNames[statsScreenIndex]}.bmp"</button
				>
			</div>

			<img src={imageData} alt="OBS Screenshot" class="max-w-full rounded" />
		</div>
	{/if}
</div>
