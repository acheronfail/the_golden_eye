<script lang="ts">
	import { onDestroy } from 'svelte';
	import SectionTitle from '$lib/components/SectionTitle.svelte';
	import { linear } from 'svelte/easing';
	import { Tween } from 'svelte/motion';
	import { backend, type RunClip } from '$lib/api';
	import YouTubeConnectButton from '$lib/components/YouTubeConnectButton.svelte';
	import { settings } from '$lib/stores/settings.svelte';
	import { youtube } from '$lib/stores/youtube.svelte';
	import { datetimeLocalForClip, renderYouTubeUploadPreview } from '$lib/utils/youtubeMetadata';

	let { clip }: { clip: RunClip } = $props();

	let upload = $derived(youtube.uploadForPath(clip.path));
	let history = $derived(youtube.historyForPath(clip.path));
	let helpOpen = $state(false);
	let initializedPath = $state<string | null>(null);
	let copied = $state(false);
	let forgetArmed = $state(false);
	let dismissedUploadErrorId = $state<string | null>(null);
	let dismissedStoreError = $state<string | null>(null);
	let copyResetTimer: ReturnType<typeof setTimeout> | null = null;
	let forgetResetTimer: ReturnType<typeof setTimeout> | null = null;
	const displayProgress = Tween.of(() => Math.max(0, Math.min(1, upload?.progressRatio ?? 0)), {
		duration: 650,
		easing: linear
	});

	$effect(() => {
		if (clip.path !== initializedPath) {
			helpOpen = false;
			initializedPath = clip.path;
		}
	});

	let progressLabel = $derived(
		upload?.progressRatio === null || upload?.progressRatio === undefined
			? null
			: `${Math.round(displayProgress.current * 100)}%`
	);
	let buttonLabel = $derived.by(() => {
		if (!youtube.loaded) return 'Loading YouTube...';
		if (!upload && history) return 'Uploaded';
		if (!upload) return 'Upload';
		if (upload.state === 'queued') return 'Queued...';
		if (upload.state === 'uploading') return progressLabel ? `Uploading ${progressLabel}...` : 'Uploading...';
		if (upload.state === 'processing') return 'Processing...';
		if (upload.state === 'uploaded') return 'Uploaded';
		return 'Upload';
	});
	let buttonDisabled = $derived(
		!youtube.loaded ||
			(!youtube.connected && !youtube.oauthConfigured) ||
			upload?.state === 'queued' ||
			upload?.state === 'uploading' ||
			upload?.state === 'processing' ||
			upload?.state === 'uploaded' ||
			(!upload && history !== null)
	);
	let openUrl = $derived(upload?.videoUrl ?? history?.videoUrl ?? null);
	let isUploaded = $derived(Boolean(openUrl && (upload?.state === 'uploaded' || (!upload && history))));
	let preview = $derived(
		renderYouTubeUploadPreview(clip, {
			titleTemplate: settings.youtubeTitleTemplate,
			descriptionTemplate: settings.youtubeDescriptionTemplate,
			visibility: settings.youtubeVisibility,
			datetimeLocal: datetimeLocalForClip(clip, typeof navigator === 'undefined' ? undefined : navigator.languages)
		})
	);
	let visibleUploadError = $derived(
		upload?.state === 'failed' && upload.id !== dismissedUploadErrorId ? (upload.error ?? 'Upload failed') : null
	);
	let visibleStoreError = $derived(youtube.error && youtube.error !== dismissedStoreError ? youtube.error : null);

	const openVideo = () => {
		if (!openUrl) return;
		void backend.openYouTubeUrl(openUrl).catch((err) => console.warn('Failed to open YouTube video', err));
	};
	const uploadVideo = () => {
		const datetimeLocal = datetimeLocalForClip(
			clip,
			typeof navigator === 'undefined' ? undefined : navigator.languages
		);
		void youtube.upload(clip.path, { datetimeLocal }).catch((err) => console.warn('Failed to upload to YouTube', err));
	};
	const forgetUpload = () => {
		void youtube.forget(clip.path).catch((err) => console.warn('Failed to forget YouTube upload', err));
	};
	const copyUrl = () => {
		if (!openUrl) return;
		void navigator.clipboard
			.writeText(openUrl)
			.then(() => {
				copied = true;
				if (copyResetTimer) clearTimeout(copyResetTimer);
				copyResetTimer = setTimeout(() => {
					copied = false;
					copyResetTimer = null;
				}, 1500);
			})
			.catch((err) => console.warn('Failed to copy YouTube URL', err));
	};
	const selectUrl = (event: Event) => {
		(event.currentTarget as HTMLInputElement).select();
	};
	const armOrForgetUpload = () => {
		if (forgetArmed) {
			forgetArmed = false;
			if (forgetResetTimer) clearTimeout(forgetResetTimer);
			forgetResetTimer = null;
			forgetUpload();
			return;
		}
		forgetArmed = true;
		if (forgetResetTimer) clearTimeout(forgetResetTimer);
		forgetResetTimer = setTimeout(() => {
			forgetArmed = false;
			forgetResetTimer = null;
		}, 4000);
	};

	onDestroy(() => {
		if (copyResetTimer) clearTimeout(copyResetTimer);
		if (forgetResetTimer) clearTimeout(forgetResetTimer);
	});
</script>

{#snippet sectionActions()}
	{#if isUploaded}
		<button
			type="button"
			class="obs-text-button cursor-pointer rounded-full obs-button-xs"
			aria-expanded={helpOpen}
			aria-label="Explain YouTube upload history"
			onclick={() => (helpOpen = !helpOpen)}>?</button
		>
	{/if}
{/snippet}

{#if youtube.enabled}
	<section class="mt-4">
		<SectionTitle title="YouTube" actions={sectionActions} class="mb-3" />
		{#if isUploaded && helpOpen}
			<div class="mb-3 grid gap-2 text-xs leading-relaxed text-(--obs-text-muted)">
				<p>
					The plugin remembers videos it uploaded from this computer and links them to the clip's file path. If a clip
					is moved, renamed, or edited with other video apps, it may not recognise it as the same video and might not
					remember that it was uploaded or not.
				</p>
				<p class="flex flex-wrap items-center gap-2">
					<span>Want to upload this clip again?</span>
					<button
						type="button"
						class={forgetArmed
							? 'obs-button cursor-pointer obs-button-danger obs-button-xs'
							: 'obs-button cursor-pointer obs-button-xs'}
						onclick={armOrForgetUpload}
					>
						{forgetArmed ? 'Click again to forget' : 'Forget upload'}
					</button>
				</p>
			</div>
		{/if}
		<div class="flex flex-col items-center gap-2 text-center">
			{#if isUploaded}
				<div class="mb-1 w-full text-left">
					<p class="font-mono text-xs text-(--obs-text)">Already uploaded to YouTube.</p>
					<p class="mt-1 text-xs obs-dim">Use the link below to copy or open the uploaded video.</p>
				</div>
			{/if}
			{#if !isUploaded && youtube.connected}
				<div class="mb-1 grid w-full gap-3 text-left">
					<div class="flex flex-wrap items-center justify-between gap-2">
						<p class="font-mono text-[11px] tracking-[0.18em] obs-dim uppercase">Preview</p>
						<div class="flex items-center gap-2">
							<a class="obs-text-button obs-button-xs" href="/options?tab=youtube">Edit templates</a>
							<button
								type="button"
								onclick={uploadVideo}
								disabled={buttonDisabled}
								class="obs-button obs-button-gold obs-button-xs disabled:cursor-not-allowed disabled:opacity-50"
							>
								{buttonLabel}
							</button>
						</div>
					</div>
					<dl class="grid gap-2.5 text-xs sm:grid-cols-[5.5rem_minmax(0,1fr)]">
						<dt class="pt-1 font-mono obs-dim">Title</dt>
						<dd class="obs-input px-3 py-2 font-mono text-[11px] leading-relaxed wrap-break-word text-(--obs-text)">
							{preview.title}
						</dd>
						<dt class="pt-1 font-mono obs-dim">Description</dt>
						<dd
							class="obs-input px-3 py-2 font-mono text-[11px] leading-relaxed wrap-break-word whitespace-pre-wrap text-(--obs-text)"
						>
							{preview.description || 'No description'}
						</dd>
						<dt class="font-mono obs-dim">Visibility</dt>
						<dd>
							<span
								class="inline-flex rounded border border-(--obs-border-soft) bg-(--obs-control) px-2 py-0.5 font-mono text-[11px] text-(--obs-text-muted) shadow-[inset_0_1px_0_var(--obs-border-soft)]"
							>
								{preview.visibilityLabel}
							</span>
						</dd>
					</dl>
				</div>
			{:else if !isUploaded}
				<p class="text-xs leading-relaxed obs-dim">Connect YouTube to upload videos.</p>
				{#if youtube.loaded}
					<YouTubeConnectButton class="mt-1 px-3 py-1.5 font-mono text-sm" />
				{/if}
			{/if}
			{#if openUrl}
				<div class="flex w-full items-center justify-center gap-2 px-2 sm:px-8">
					<input
						class="obs-input min-w-0 flex-1 truncate border-(--obs-border-soft) px-3 py-1.5 text-center font-mono text-xs shadow-[inset_0_1px_0_var(--obs-border-soft)]"
						readonly
						value={openUrl}
						aria-label="YouTube video URL"
						onclick={selectUrl}
						onfocus={selectUrl}
					/>
					<button type="button" class="obs-button w-17 obs-button-xs" onclick={copyUrl}
						>{copied ? 'Copied' : 'Copy'}</button
					>
					<button type="button" class="obs-button obs-button-xs" onclick={openVideo}>Open</button>
				</div>
			{/if}
			{#if upload?.state === 'uploading' && upload.progressRatio !== null}
				<div class="h-2 w-full max-w-sm overflow-hidden rounded bg-black/30">
					<div
						class="h-full w-(--upload-progress) bg-(--obs-gold)"
						style:--upload-progress={`${Math.max(0, Math.min(100, displayProgress.current * 100))}%`}
					></div>
				</div>
			{/if}
			{#if visibleUploadError}
				<div class="mt-1 w-full rounded obs-alert-error px-3 py-2 text-left">
					<div class="flex items-start justify-between gap-3">
						<p class="text-xs font-semibold obs-alert-error-title">YouTube upload failed</p>
						<button
							type="button"
							class="obs-text-button cursor-pointer obs-button-xs"
							aria-label="Dismiss YouTube upload error"
							onclick={() => (dismissedUploadErrorId = upload?.id ?? null)}>×</button
						>
					</div>
					<p
						class="mt-1 font-mono text-[11px] leading-relaxed wrap-break-word whitespace-pre-wrap obs-alert-error-body"
					>
						{visibleUploadError}
					</p>
				</div>
			{:else if youtube.loaded && !youtube.oauthConfigured}
				<p class="text-xs text-(--obs-danger)">YouTube OAuth is not configured in this build.</p>
			{/if}
			{#if visibleStoreError}
				<div class="mt-1 w-full rounded obs-alert-error px-3 py-2 text-left">
					<div class="flex items-start justify-between gap-3">
						<p class="text-xs font-semibold obs-alert-error-title">YouTube error</p>
						<button
							type="button"
							class="obs-text-button cursor-pointer obs-button-xs"
							aria-label="Dismiss YouTube error"
							onclick={() => (dismissedStoreError = youtube.error)}>×</button
						>
					</div>
					<p
						class="mt-1 font-mono text-[11px] leading-relaxed wrap-break-word whitespace-pre-wrap obs-alert-error-body"
					>
						{visibleStoreError}
					</p>
				</div>
			{/if}
		</div>
	</section>
{/if}
