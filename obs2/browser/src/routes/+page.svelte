<script lang="ts">
	import { apiUrl } from '$lib/api';

	let imageData = $state<string | null>(null);
	let sources = $state<{ name: string; id: string }[]>([]);

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
					<li>
						{source.name}
						{#if ['screen_capture', 'macos-avcapture-fast'].includes(source.id)}
							<button
								class="ml-2 rounded bg-green-500 px-2 py-1 text-white hover:bg-green-600"
								onclick={getScreenshot(source.name)}>get screenshot</button
							>
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
