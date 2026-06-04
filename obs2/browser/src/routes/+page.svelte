<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import * as obs from '$lib/obs';

	let monitoring = $state(false);
	let imageData = $state<string | null>(null);
	let fps = $state(0);

	const frameTimes: number[] = [];
	$effect(() => {
		if (monitoring) {

			const update = async (timestamp: number) => {
				if (!monitoring) return;

				while (frameTimes[0] < timestamp - 1_000) {
					frameTimes.shift();
				}
				frameTimes.push(timestamp);
				fps = frameTimes.length;

				try {
					const frame = await obs.getFrame();
					if (monitoring) {
						imageData = frame;
					}
				} finally {
					if (monitoring) {
						requestAnimationFrame(update);
					}
				}
			};

			update(0);
		}
	});

	onMount(() => {
		// Redirect to OBS connection page if needed
		obs.testConnection().then((connected) => {
			if (connected) {
				monitoring = import.meta.env.DEV;
			} else {
				goto('/obs');
			}
		});

		return () => {
			monitoring = false;
			imageData = null;
			obs.disconnect();
		};
	});
</script>

<!-- TODO: screen to set ROIs for goldeneye -->
<!-- TODO: get image screenshot from OBS, and then display -->

<div>
	<h1 class="mb-4 text-2xl font-bold">Welcome to Goldeneye!</h1>
	<p class="mb-4">
		This is the main dashboard. Here you can set up your ROIs and view the live feed from OBS.
	</p>

	<button
		class="mb-4 rounded bg-blue-500 px-4 py-2 font-semibold text-white hover:bg-blue-600"
		onclick={() => (monitoring = !monitoring)}
	>
		{monitoring ? 'Stop Monitoring' : 'Start Monitoring'}
	</button>

	{#if imageData}
		<span>FPS: {fps}</span>
		<div class="m-4 p-2 max-w-4xl border rounded shadow-md">
			<h2 class="mb-2 text-xl font-semibold">Live Feed from OBS:</h2>
			<img src={imageData} alt="Live feed from OBS" class="w-full rounded-lg shadow-md" />
		</div>
	{/if}
</div>
