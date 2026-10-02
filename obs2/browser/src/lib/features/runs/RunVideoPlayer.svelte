<script lang="ts">
	let {
		src,
		fileBrowserLabel,
		onReveal,
		busy = false
	}: {
		src: string;
		fileBrowserLabel: string;
		onReveal: () => void;
		busy?: boolean;
	} = $props();

	let failure = $state<{ src: string; kind: 'video' | 'load' } | null>(null);
	let problem = $derived(failure?.src === src ? failure.kind : null);

	function checkVideo(event: Event) {
		const video = event.currentTarget as HTMLVideoElement;
		// Wait for loaded data: zero dimensions during initial loading are normal.
		if (video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA) return;
		failure = video.videoWidth > 0 && video.videoHeight > 0 ? null : { src, kind: 'video' };
	}

	function playbackError(event: Event) {
		const video = event.currentTarget as HTMLVideoElement;
		const code = video.error?.code;
		if (!code || code === MediaError.MEDIA_ERR_ABORTED) return;
		failure = { src, kind: code === MediaError.MEDIA_ERR_NETWORK ? 'load' : 'video' };
	}
</script>

{#key src}
	<!-- svelte-ignore a11y_media_has_caption -->
	<video
		src={src || undefined}
		controls
		preload="auto"
		class="aspect-video w-full obs-preview"
		onloadeddata={checkVideo}
		oncanplay={checkVideo}
		onresize={checkVideo}
		onerror={playbackError}
		onloadstart={() => (failure = null)}
	></video>
{/key}

{#if problem}
	<div
		role="alert"
		class="mt-3 rounded border border-[color-mix(in_srgb,var(--obs-danger),var(--obs-border)_45%)] bg-(--obs-danger-surface) px-4 py-3"
	>
		<p class="text-sm font-semibold obs-alert-error-title">Video preview unavailable</p>
		<p class="mt-1 text-xs obs-alert-error-body">
			{#if problem === 'video'}
				OBS's built-in browser may not support this clip's video format.
			{:else}
				The clip could not be loaded. Check that the file is still available.
			{/if}
			Reveal the file to open it in a video player.
		</p>
		<button
			type="button"
			class="mt-3 obs-text-button h-9 w-44 max-w-full px-3 py-2 font-mono text-xs"
			onclick={onReveal}
			disabled={busy}
		>
			{fileBrowserLabel}
		</button>
	</div>
{/if}
