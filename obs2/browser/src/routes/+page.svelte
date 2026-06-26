<script lang="ts">
	import { apiUrl } from '$lib/api';

	let imageData = $state<string | null>(null);
	let sources = $state<{ name: string; id: string }[]>([]);
	let monitoring = $state(false);

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
	<p class="mb-4">
		This is the main dashboard. Here you can set up your ROIs and view the live feed from OBS.
	</p>

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
		<p class="mb-4 text-gray-500">
			No sources found. Please make sure OBS is running and has sources set up.
		</p>
	{/if}

	{#if imageData}
		<div class="mb-4">
			<h2 class="mb-2 text-xl font-semibold">Screenshot:</h2>
			<img src={imageData} alt="OBS Screenshot" class="max-w-full rounded" />
		</div>
	{/if}
</div>
