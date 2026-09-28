<script lang="ts">
	import type { YouTubeUploadStatus } from '$lib/api';
	import { youtube } from '$lib/stores/youtube.svelte';

	let { upload }: { upload: YouTubeUploadStatus } = $props();
	let pending = $derived(upload.state === 'cancelling' || youtube.cancellingUploadIds.includes(upload.id));
</script>

{#if upload.state === 'queued' || upload.state === 'uploading' || upload.state === 'cancelling'}
	<div class="text-xs">
		<button
			type="button"
			class="obs-text-button obs-button-xs disabled:cursor-not-allowed disabled:opacity-50"
			aria-label={`Cancel upload: ${upload.title || upload.fileName}`}
			disabled={pending}
			onclick={() => void youtube.cancelUpload(upload.id)}>{pending ? 'Cancelling…' : 'Cancel'}</button
		>
		{#if youtube.uploadCancelErrors[upload.id]}
			<p role="alert" class="mt-1 text-(--obs-danger)">{youtube.uploadCancelErrors[upload.id]}</p>
		{/if}
	</div>
{/if}
