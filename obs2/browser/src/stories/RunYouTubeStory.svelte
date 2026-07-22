<script lang="ts">
	import type { RunClip, YouTubeStatus } from '$lib/api';
	import RunYouTubeSection from '$lib/components/RunYouTubeSection.svelte';
	import { youtube } from '$lib/stores/youtube.svelte';

	let {
		clip,
		status,
		connecting = false,
		error = null
	}: {
		clip: RunClip;
		status: YouTubeStatus;
		connecting?: boolean;
		error?: string | null;
	} = $props();

	$effect(() => {
		youtube.applyStatus(status);
		youtube.connecting = connecting;
		youtube.cancelling = false;
		youtube.disconnecting = false;
		youtube.error = error;
	});
</script>

<main class="mx-auto w-full max-w-5xl px-4 py-8">
	<RunYouTubeSection {clip} />
</main>
