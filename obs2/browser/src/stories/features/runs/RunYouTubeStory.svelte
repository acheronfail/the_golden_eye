<script lang="ts">
	import type { RunClip, YouTubeStatus } from '$lib/api';
	import RunYouTubeSection from '$lib/features/runs/RunYouTubeSection.svelte';
	import { youtube } from '$lib/stores/youtube.svelte';

	let {
		clip,
		status,
		connecting = false,
		cancelError = null,
		error = null
	}: {
		clip: RunClip;
		status: YouTubeStatus;
		connecting?: boolean;
		cancelError?: string | null;
		error?: string | null;
	} = $props();

	$effect(() => {
		youtube.applyStatus(status);
		youtube.connecting = connecting;
		youtube.cancelling = false;
		youtube.disconnecting = false;
		youtube.error = error;
		youtube.cancellingUploadIds = [];
		youtube.uploadCancelErrors = cancelError && status.uploads[0] ? { [status.uploads[0].id]: cancelError } : {};
	});
</script>

<main class="mx-auto w-full max-w-5xl px-4 py-8">
	<RunYouTubeSection {clip} />
</main>
