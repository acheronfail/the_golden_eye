<script lang="ts">
	import type { YouTubeStatus } from '$lib/api';
	import OptionsYouTube from '$lib/components/OptionsYouTube.svelte';
	import { youtube } from '$lib/stores/youtube.svelte';

	let {
		status,
		connecting = false,
		cancelling = false,
		disconnecting = false,
		error = null
	}: {
		status: YouTubeStatus;
		connecting?: boolean;
		cancelling?: boolean;
		disconnecting?: boolean;
		error?: string | null;
	} = $props();

	$effect(() => {
		youtube.applyStatus(status);
		youtube.connecting = connecting;
		youtube.cancelling = cancelling;
		youtube.disconnecting = disconnecting;
		youtube.error = error;
	});
</script>

<main class="mx-auto grid w-full max-w-2xl gap-4 px-4 py-8 sm:px-6">
	<div>
		<h1 class="obs-heading text-2xl font-semibold">Options</h1>
		<p class="obs-subtitle mt-1 text-sm">YouTube connection and upload defaults.</p>
	</div>
	<OptionsYouTube />
</main>
