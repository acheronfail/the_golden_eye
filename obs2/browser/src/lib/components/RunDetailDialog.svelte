<script lang="ts">
	import { onDestroy } from 'svelte';
	import { linear } from 'svelte/easing';
	import { tweened } from 'svelte/motion';
	import { backend, type EditableRunMetadata, type RunClip } from '$lib/api';
	import Select from '$lib/components/Select.svelte';
	import { settings } from '$lib/stores/settings.svelte';
	import {
		DIFFICULTY_OPTIONS,
		LANGUAGE_OPTIONS,
		STATUS_OPTIONS,
		fileRows,
		runDetail,
		type RunDetailView
	} from '$lib/utils/runsView';
	import { datetimeLocalForClip, renderYouTubeUploadPreview } from '$lib/utils/youtubeMetadata';
	import { youtube } from '$lib/stores/youtube.svelte';
	import YouTubeConnectButton from '$lib/components/YouTubeConnectButton.svelte';

	let {
		clip,
		metadataDraft = $bindable(),
		view
	}: {
		clip: RunClip | null;
		metadataDraft: EditableRunMetadata | null;
		view: RunDetailView;
	} = $props();

	let youtubeUpload = $derived(clip ? youtube.uploadForPath(clip.path) : null);
	let youtubeHistory = $derived(clip ? youtube.historyForPath(clip.path) : null);

	let youtubeHelpOpen = $state(false);
	let youtubeHelpInitializedForClip = $state<string | null>(null);
	let youtubeCopied = $state(false);
	let youtubeForgetArmed = $state(false);
	let youtubeDismissedUploadErrorId = $state<string | null>(null);
	let youtubeDismissedStoreError = $state<string | null>(null);
	let youtubeCopyResetTimer: ReturnType<typeof setTimeout> | null = null;
	let youtubeForgetResetTimer: ReturnType<typeof setTimeout> | null = null;
	let metadataSaveTimer: ReturnType<typeof setTimeout> | null = null;
	const youtubeDisplayProgress = tweened(0, { duration: 650, easing: linear });
	let youtubeDisplayProgressRatio = $state(0);

	$effect(() => {
		const progress = youtubeUpload?.progressRatio ?? 0;
		void youtubeDisplayProgress.set(Math.max(0, Math.min(1, progress)));
	});

	$effect(() => {
		const unsubscribe = youtubeDisplayProgress.subscribe((value) => {
			youtubeDisplayProgressRatio = value;
		});
		return unsubscribe;
	});

	let youtubeProgressLabel = $derived.by(() => {
		if (!youtubeUpload || youtubeUpload.progressRatio === null) return null;
		return `${Math.round(youtubeDisplayProgressRatio * 100)}%`;
	});
	let youtubeButtonLabel = $derived.by(() => {
		if (!youtube.loaded) return 'Loading YouTube...';
		if (!youtubeUpload && youtubeHistory) return 'Uploaded';
		if (!youtubeUpload) return 'Upload';
		if (youtubeUpload.state === 'queued') return 'Queued...';
		if (youtubeUpload.state === 'uploading')
			return youtubeProgressLabel ? `Uploading ${youtubeProgressLabel}...` : 'Uploading...';
		if (youtubeUpload.state === 'processing') return 'Processing...';
		if (youtubeUpload.state === 'uploaded') return 'Uploaded';
		return 'Upload';
	});
	let youtubeButtonDisabled = $derived(
		!youtube.loaded ||
			(!youtube.connected && !youtube.oauthConfigured) ||
			youtubeUpload?.state === 'queued' ||
			youtubeUpload?.state === 'uploading' ||
			youtubeUpload?.state === 'processing' ||
			youtubeUpload?.state === 'uploaded' ||
			(!youtubeUpload && youtubeHistory !== null)
	);
	let youtubeOpenUrl = $derived(youtubeUpload?.videoUrl ?? youtubeHistory?.videoUrl ?? null);
	let youtubeIsUploaded = $derived(
		Boolean(youtubeOpenUrl && (youtubeUpload?.state === 'uploaded' || (!youtubeUpload && youtubeHistory)))
	);
	let youtubeUploadPreview = $derived.by(() => {
		if (!clip) return null;
		return renderYouTubeUploadPreview(clip, {
			titleTemplate: settings.youtubeTitleTemplate,
			descriptionTemplate: settings.youtubeDescriptionTemplate,
			visibility: settings.youtubeVisibility,
			datetimeLocal: datetimeLocalForClip(clip, typeof navigator === 'undefined' ? undefined : navigator.languages)
		});
	});
	let visibleYoutubeUploadError = $derived(
		youtubeUpload?.state === 'failed' && youtubeUpload.id !== youtubeDismissedUploadErrorId
			? (youtubeUpload.error ?? 'Upload failed')
			: null
	);
	let visibleYoutubeError = $derived(
		youtube.error && youtube.error !== youtubeDismissedStoreError ? youtube.error : null
	);

	const scheduleMetadataSave = (debounceMs = 0) => {
		if (metadataSaveTimer) clearTimeout(metadataSaveTimer);
		metadataSaveTimer = setTimeout(() => {
			metadataSaveTimer = null;
			view.actions.saveMetadata();
		}, debounceMs);
	};
	const saveMetadataNow = () => {
		if (metadataSaveTimer) {
			clearTimeout(metadataSaveTimer);
			metadataSaveTimer = null;
		}
		view.actions.saveMetadata();
	};
	const normalizeAndSaveMetadataNow = () => {
		view.actions.normalizeDraftTime();
		saveMetadataNow();
	};
	onDestroy(() => {
		if (metadataSaveTimer) clearTimeout(metadataSaveTimer);
	});

	const openYoutubeVideo = () => {
		if (!youtubeOpenUrl) return;
		void backend.openYouTubeUrl(youtubeOpenUrl).catch((err) => console.warn('Failed to open YouTube video', err));
	};
	const uploadToYouTube = () => {
		if (!clip) return;
		const datetimeLocal = datetimeLocalForClip(
			clip,
			typeof navigator === 'undefined' ? undefined : navigator.languages
		);
		void youtube.upload(clip.path, { datetimeLocal }).catch((err) => console.warn('Failed to upload to YouTube', err));
	};
	const forgetYouTubeUpload = () => {
		if (!clip) return;
		void youtube.forget(clip.path).catch((err) => console.warn('Failed to forget YouTube upload', err));
	};
	const copyYouTubeUrl = () => {
		if (!youtubeOpenUrl) return;
		void navigator.clipboard
			.writeText(youtubeOpenUrl)
			.then(() => {
				youtubeCopied = true;
				if (youtubeCopyResetTimer) clearTimeout(youtubeCopyResetTimer);
				youtubeCopyResetTimer = setTimeout(() => {
					youtubeCopied = false;
					youtubeCopyResetTimer = null;
				}, 1500);
			})
			.catch((err) => console.warn('Failed to copy YouTube URL', err));
	};
	$effect(() => {
		const path = clip?.path ?? null;
		if (path && youtubeHelpInitializedForClip !== path) {
			youtubeHelpOpen = false;
			youtubeHelpInitializedForClip = path;
		}
	});

	const selectYouTubeUrl = (event: Event) => {
		(event.currentTarget as HTMLInputElement).select();
	};
	const armOrForgetYouTubeUpload = () => {
		if (youtubeForgetArmed) {
			youtubeForgetArmed = false;
			if (youtubeForgetResetTimer) {
				clearTimeout(youtubeForgetResetTimer);
				youtubeForgetResetTimer = null;
			}
			forgetYouTubeUpload();
			return;
		}
		youtubeForgetArmed = true;
		if (youtubeForgetResetTimer) clearTimeout(youtubeForgetResetTimer);
		youtubeForgetResetTimer = setTimeout(() => {
			youtubeForgetArmed = false;
			youtubeForgetResetTimer = null;
		}, 4000);
	};
</script>

{#if clip}
	<div class="obs-overlay fixed inset-0 z-50 flex items-center justify-center p-4">
		<button
			type="button"
			aria-label="Close run viewer"
			class="absolute inset-0 cursor-default"
			onclick={view.actions.close}
		></button>
		<dialog
			open
			aria-label="Run video"
			class="obs-dialog relative z-10 m-0 max-h-full w-full max-w-5xl overflow-hidden rounded p-0"
		>
			<header class="obs-dialog-header px-4 py-3">
				<h2 class="obs-heading truncate text-lg font-semibold" title={clip.fileName}>{clip.fileName}</h2>
				<p class="obs-dim mt-1 truncate font-mono text-xs" title={runDetail(clip)}>{runDetail(clip)}</p>
			</header>

			<div class="max-h-[calc(100vh-9rem)] overflow-y-auto p-4">
				<div class="mb-4 flex flex-wrap justify-end gap-2">
					<button
						type="button"
						onclick={view.actions.delete}
						disabled={view.modal.busy !== null}
						class="obs-text-button obs-button-danger px-2 py-1 font-mono text-xs">delete</button
					>
					<button
						type="button"
						onclick={view.actions.reveal}
						disabled={view.modal.busy !== null}
						class="obs-text-button px-2 py-1 font-mono text-xs"
					>
						{view.display.fileBrowserLabel}
					</button>
					<button
						type="button"
						onclick={view.actions.rename}
						disabled={view.modal.busy !== null}
						class="obs-text-button px-2 py-1 font-mono text-xs">rename</button
					>
					<button type="button" onclick={view.actions.close} class="obs-text-button px-2 py-1 font-mono text-xs"
						>close</button
					>
				</div>
				<!-- svelte-ignore a11y_media_has_caption -->
				<video src={backend.runVideoUrl(clip.path)} controls class="obs-preview aspect-video w-full"></video>

				{#if youtube.enabled}
					<section class="mt-4">
						<div class="mb-3 flex items-center justify-between gap-3 border-b border-(--obs-border) pb-2">
							<h3 class="font-mono text-xs font-semibold tracking-[0.2em] text-(--obs-text-muted) uppercase">
								YouTube
							</h3>
							{#if youtubeIsUploaded}
								<button
									type="button"
									class="obs-text-button obs-button-xs cursor-pointer rounded-full"
									aria-expanded={youtubeHelpOpen}
									aria-label="Explain YouTube upload history"
									onclick={() => (youtubeHelpOpen = !youtubeHelpOpen)}>?</button
								>
							{/if}
						</div>
						{#if youtubeIsUploaded && youtubeHelpOpen}
							<div class="mb-3 grid gap-2 text-xs leading-relaxed text-(--obs-text-muted)">
								<p>
									The plugin remembers videos it uploaded from this computer and links them to the clip's file path. If
									a clip is moved, renamed, or edited with other video apps, it may not recognise it as the same video
									and might not remember that it was uploaded or not.
								</p>
								<p class="flex flex-wrap items-center gap-2">
									<span>Want to upload this clip again?</span>
									<button
										type="button"
										class={youtubeForgetArmed
											? 'obs-button obs-button-danger obs-button-xs cursor-pointer'
											: 'obs-button obs-button-xs cursor-pointer'}
										onclick={armOrForgetYouTubeUpload}
									>
										{youtubeForgetArmed ? 'Click again to forget' : 'Forget upload'}
									</button>
								</p>
							</div>
						{/if}
						<div class="flex flex-col items-center gap-2 text-center">
							{#if youtubeIsUploaded}
								<div class="mb-1 w-full text-left">
									<p class="font-mono text-xs text-(--obs-text)">Already uploaded to YouTube.</p>
									<p class="obs-dim mt-1 text-xs">Use the link below to copy or open the uploaded video.</p>
								</div>
							{/if}
							{#if !youtubeIsUploaded && youtube.connected && youtubeUploadPreview}
								<div class="mb-1 grid w-full gap-3 text-left">
									<div class="flex flex-wrap items-center justify-between gap-2">
										<p class="obs-dim font-mono text-[11px] tracking-[0.18em] uppercase">Upload preview</p>
										<div class="flex items-center gap-2">
											<a class="obs-text-button obs-button-xs" href="/options?tab=youtube">Edit templates</a>
											<button
												type="button"
												onclick={uploadToYouTube}
												disabled={youtubeButtonDisabled}
												class="obs-button obs-button-gold obs-button-xs disabled:cursor-not-allowed disabled:opacity-50"
											>
												{youtubeButtonLabel}
											</button>
										</div>
									</div>
									<dl class="grid gap-2.5 text-xs sm:grid-cols-[5.5rem_minmax(0,1fr)]">
										<dt class="obs-dim pt-1 font-mono">Title</dt>
										<dd
											class="obs-input px-3 py-2 font-mono text-[11px] leading-relaxed wrap-break-word text-(--obs-text)"
										>
											{youtubeUploadPreview.title}
										</dd>
										<dt class="obs-dim pt-1 font-mono">Description</dt>
										<dd
											class="obs-input px-3 py-2 font-mono text-[11px] leading-relaxed wrap-break-word whitespace-pre-wrap text-(--obs-text)"
										>
											{youtubeUploadPreview.description || 'No description'}
										</dd>
										<dt class="obs-dim font-mono">Visibility</dt>
										<dd>
											<span
												class="inline-flex rounded border border-(--obs-border-soft) bg-(--obs-control) px-2 py-0.5 font-mono text-[11px] text-(--obs-text-muted) shadow-[inset_0_1px_0_var(--obs-border-soft)]"
											>
												{youtubeUploadPreview.visibilityLabel}
											</span>
										</dd>
									</dl>
								</div>
							{:else if !youtubeIsUploaded}
								<p class="obs-dim text-xs leading-relaxed">Connect YouTube to upload videos.</p>
								{#if youtube.loaded}
									<YouTubeConnectButton class="mt-1 px-3 py-1.5 font-mono text-sm" />
								{/if}
							{/if}
							{#if youtubeOpenUrl}
								<div class="flex w-full items-center justify-center gap-2 px-2 sm:px-8">
									<input
										class="obs-input min-w-0 flex-1 truncate border-(--obs-border-strong) px-3 py-1.5 text-center font-mono text-xs shadow-[inset_0_1px_0_var(--obs-border-soft)]"
										readonly
										value={youtubeOpenUrl}
										aria-label="YouTube video URL"
										onclick={selectYouTubeUrl}
										onfocus={selectYouTubeUrl}
									/>
									<button type="button" class="obs-button obs-button-xs w-17" onclick={copyYouTubeUrl}
										>{youtubeCopied ? 'Copied' : 'Copy'}</button
									>
									<button type="button" class="obs-button obs-button-xs" onclick={openYoutubeVideo}>Open</button>
								</div>
							{/if}
							{#if youtubeUpload?.state === 'uploading' && youtubeUpload.progressRatio !== null}
								<div class="h-2 w-full max-w-sm overflow-hidden rounded bg-black/30">
									<div
										class="h-full bg-(--obs-gold)"
										style={`width: ${Math.max(0, Math.min(100, youtubeDisplayProgressRatio * 100))}%`}
									></div>
								</div>
							{/if}
							{#if visibleYoutubeUploadError}
								<div class="obs-alert-error mt-1 w-full rounded px-3 py-2 text-left">
									<div class="flex items-start justify-between gap-3">
										<p class="obs-alert-error-title text-xs font-semibold">YouTube upload failed</p>
										<button
											type="button"
											class="obs-text-button obs-button-xs cursor-pointer"
											aria-label="Dismiss YouTube upload error"
											onclick={() => (youtubeDismissedUploadErrorId = youtubeUpload?.id ?? null)}>×</button
										>
									</div>
									<p
										class="obs-alert-error-body mt-1 font-mono text-[11px] leading-relaxed wrap-break-word whitespace-pre-wrap"
									>
										{visibleYoutubeUploadError}
									</p>
								</div>
							{:else if youtube.loaded && !youtube.oauthConfigured}
								<p class="text-xs text-(--obs-danger)">YouTube OAuth is not configured in this build.</p>
							{/if}
							{#if visibleYoutubeError}
								<div class="obs-alert-error mt-1 w-full rounded px-3 py-2 text-left">
									<div class="flex items-start justify-between gap-3">
										<p class="obs-alert-error-title text-xs font-semibold">YouTube error</p>
										<button
											type="button"
											class="obs-text-button obs-button-xs cursor-pointer"
											aria-label="Dismiss YouTube error"
											onclick={() => (youtubeDismissedStoreError = youtube.error)}>×</button
										>
									</div>
									<p
										class="obs-alert-error-body mt-1 font-mono text-[11px] leading-relaxed wrap-break-word whitespace-pre-wrap"
									>
										{visibleYoutubeError}
									</p>
								</div>
							{/if}
						</div>
					</section>
				{/if}

				{#if view.modal.error}
					<div class="obs-alert-error mt-4 rounded px-4 py-3">
						<p class="obs-alert-error-title text-sm font-semibold">Run update failed</p>
						<p class="obs-alert-error-body mt-1 font-mono text-xs">{view.modal.error}</p>
					</div>
				{/if}

				{#if metadataDraft}
					<section class="mt-4">
						<h3
							class="mb-3 border-b border-(--obs-border) pb-2 font-mono text-xs font-semibold tracking-[0.2em] text-(--obs-text-muted) uppercase"
						>
							Metadata
						</h3>
						<div class="grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
							<label class="flex min-w-0 flex-col gap-1">
								<span class="obs-dim font-mono text-xs">Level</span>
								<Select
									class="w-full"
									placeholder="select level"
									bind:value={metadataDraft.level}
									options={view.display.levelOptions}
									onChange={() => scheduleMetadataSave()}
								/>
							</label>
							<label class="flex min-w-0 flex-col gap-1">
								<span class="obs-dim font-mono text-xs">ROM language</span>
								<Select
									class="w-full"
									placeholder="select language"
									bind:value={metadataDraft.romLanguage}
									options={LANGUAGE_OPTIONS}
									onChange={() => scheduleMetadataSave()}
								/>
							</label>
							<label class="flex min-w-0 flex-col gap-1">
								<span class="obs-dim font-mono text-xs">Time</span>
								<input
									class="obs-input px-3 py-2 font-mono"
									bind:value={metadataDraft.time}
									oninput={() => scheduleMetadataSave(650)}
									onblur={normalizeAndSaveMetadataNow}
									inputmode="numeric"
									pattern="[0-9]+:[0-5][0-9]"
									placeholder="mm:ss"
								/>
							</label>
							<label class="flex min-w-0 flex-col gap-1">
								<span class="obs-dim font-mono text-xs">Difficulty</span>
								<Select
									class="w-full"
									placeholder="select difficulty"
									bind:value={metadataDraft.difficulty}
									options={DIFFICULTY_OPTIONS}
									onChange={() => scheduleMetadataSave()}
								/>
							</label>
							<label class="flex min-w-0 flex-col gap-1">
								<span class="obs-dim font-mono text-xs">Status</span>
								<Select
									class="w-full"
									placeholder="select status"
									bind:value={metadataDraft.status}
									options={STATUS_OPTIONS}
									onChange={() => scheduleMetadataSave()}
								/>
							</label>
						</div>
					</section>
				{/if}

				<dl class="mt-4 grid grid-cols-1 gap-x-4 gap-y-2 text-sm sm:grid-cols-[9rem_minmax(0,1fr)]">
					{#each fileRows(clip).filter((row) => row.value) as row}
						<dt class="obs-dim font-mono text-xs">{row.label}</dt>
						<dd class="obs-muted min-w-0 wrap-break-word">{row.value}</dd>
					{/each}
				</dl>
			</div>
		</dialog>
	</div>
{/if}
