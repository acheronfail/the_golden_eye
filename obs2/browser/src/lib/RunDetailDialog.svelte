<script lang="ts">
	import { onDestroy } from 'svelte';
	import { linear } from 'svelte/easing';
	import { tweened } from 'svelte/motion';
	import {
		backend,
		type EditableRunMetadata,
		type RunClip,
		type YouTubeUploadHistoryEntry,
		type YouTubeUploadStatus
	} from '$lib/api';
	import { Select, settings, type SelectOption } from '$lib';
	import { DIFFICULTY_OPTIONS, LANGUAGE_OPTIONS, STATUS_OPTIONS, fileRows, runDetail } from '$lib/runsView';
	import { datetimeLocalForClip, renderYouTubeUploadPreview } from '$lib/youtubeMetadata';
	import YouTubeConnectButton from '$lib/YouTubeConnectButton.svelte';

	let {
		clip,
		metadataDraft = $bindable(),
		modalError,
		modalBusy,
		fileBrowserLabel,
		levelOptions,
		close,
		deleteRun,
		revealRun,
		renameRun,
		saveMetadata,
		normalizeDraftTime,
		youtubeEnabled,
		youtubeConnected,
		youtubeOAuthConfigured,
		youtubeLoaded,
		youtubeUpload,
		youtubeHistory,
		youtubeError,
		uploadToYouTube,
		forgetYouTubeUpload
	}: {
		clip: RunClip | null;
		metadataDraft: EditableRunMetadata | null;
		modalError: string | null;
		modalBusy: string | null;
		fileBrowserLabel: string;
		levelOptions: SelectOption[];
		close: () => void;
		deleteRun: () => void;
		revealRun: () => void;
		renameRun: () => void;
		saveMetadata: () => void;
		normalizeDraftTime: () => void;
		youtubeEnabled: boolean;
		youtubeConnected: boolean;
		youtubeOAuthConfigured: boolean;
		youtubeLoaded: boolean;
		youtubeUpload: YouTubeUploadStatus | null;
		youtubeHistory: YouTubeUploadHistoryEntry | null;
		youtubeError: string | null;
		uploadToYouTube: () => void;
		forgetYouTubeUpload: () => void;
	} = $props();

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
		if (!youtubeLoaded) return 'Loading YouTube...';
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
		!youtubeLoaded ||
			(!youtubeConnected && !youtubeOAuthConfigured) ||
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
	let visibleYoutubeError = $derived(youtubeError && youtubeError !== youtubeDismissedStoreError ? youtubeError : null);

	const scheduleMetadataSave = (debounceMs = 0) => {
		if (metadataSaveTimer) clearTimeout(metadataSaveTimer);
		metadataSaveTimer = setTimeout(() => {
			metadataSaveTimer = null;
			saveMetadata();
		}, debounceMs);
	};
	const saveMetadataNow = () => {
		if (metadataSaveTimer) {
			clearTimeout(metadataSaveTimer);
			metadataSaveTimer = null;
		}
		saveMetadata();
	};
	const normalizeAndSaveMetadataNow = () => {
		normalizeDraftTime();
		saveMetadataNow();
	};
	onDestroy(() => {
		if (metadataSaveTimer) clearTimeout(metadataSaveTimer);
	});

	const openYoutubeVideo = () => {
		if (!youtubeOpenUrl) return;
		void backend.openYouTubeUrl(youtubeOpenUrl).catch((err) => console.warn('Failed to open YouTube video', err));
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
		<button type="button" aria-label="Close run viewer" class="absolute inset-0 cursor-default" onclick={close}
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
						onclick={deleteRun}
						disabled={modalBusy !== null}
						class="obs-text-button obs-button-danger px-2 py-1 font-mono text-xs">delete</button
					>
					<button
						type="button"
						onclick={revealRun}
						disabled={modalBusy !== null}
						class="obs-text-button px-2 py-1 font-mono text-xs"
					>
						{fileBrowserLabel}
					</button>
					<button
						type="button"
						onclick={renameRun}
						disabled={modalBusy !== null}
						class="obs-text-button px-2 py-1 font-mono text-xs">rename</button
					>
					<button type="button" onclick={close} class="obs-text-button px-2 py-1 font-mono text-xs">close</button>
				</div>
				<!-- svelte-ignore a11y_media_has_caption -->
				<video src={backend.runVideoUrl(clip.path)} controls class="obs-preview aspect-video w-full"></video>

				{#if youtubeEnabled}
					<section class="obs-subpanel mt-4 rounded px-4 py-3">
						<div class="mb-3 flex items-center justify-between gap-3">
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
								<div
									class="mb-1 w-full rounded border border-(--obs-border) bg-(--obs-bg-elevated) px-3 py-2 text-left shadow-[inset_0_1px_0_var(--obs-border-soft)]"
								>
									<p class="font-mono text-xs text-(--obs-text)">Already uploaded to YouTube.</p>
									<p class="obs-dim mt-1 text-xs">Use the link below to copy or open the uploaded video.</p>
								</div>
							{/if}
							{#if !youtubeIsUploaded && youtubeUploadPreview}
								<div
									class="mb-1 grid w-full gap-3 rounded border border-(--obs-border) bg-(--obs-bg-elevated) p-3 text-left shadow-[inset_0_1px_0_var(--obs-border-soft)]"
								>
									<div class="flex flex-wrap items-center justify-between gap-2">
										<p class="obs-dim font-mono text-[11px] tracking-[0.18em] uppercase">Upload preview</p>
										<a class="obs-text-button obs-button-xs" href="/options?tab=youtube">Edit templates</a>
									</div>
									<dl class="grid gap-2.5 text-xs sm:grid-cols-[5.5rem_minmax(0,1fr)]">
										<dt class="obs-dim pt-1 font-mono">Title</dt>
										<dd
											class="rounded border border-(--obs-border) bg-(--obs-panel) px-2 py-1.5 font-mono text-[11px] leading-relaxed wrap-break-word text-(--obs-text) shadow-[inset_0_1px_2px_rgb(0_0_0/24%)]"
										>
											{youtubeUploadPreview.title}
										</dd>
										<dt class="obs-dim pt-1 font-mono">Description</dt>
										<dd
											class="rounded border border-(--obs-border) bg-(--obs-panel) px-2 py-1.5 font-mono text-[11px] leading-relaxed wrap-break-word whitespace-pre-wrap text-(--obs-text) shadow-[inset_0_1px_2px_rgb(0_0_0/24%)]"
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
								<div class="flex flex-wrap items-center justify-center gap-2">
									{#if youtubeLoaded && !youtubeConnected}
										<YouTubeConnectButton class="px-3 py-1.5 font-mono text-sm" />
									{:else}
										<button
											type="button"
											onclick={uploadToYouTube}
											disabled={youtubeButtonDisabled}
											class="obs-button obs-button-gold px-3 py-1.5 font-mono text-sm disabled:cursor-not-allowed disabled:opacity-50"
										>
											{youtubeButtonLabel}
										</button>
									{/if}
								</div>
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
							{:else if youtubeLoaded && !youtubeOAuthConfigured}
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
											onclick={() => (youtubeDismissedStoreError = youtubeError)}>×</button
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

				{#if modalError}
					<div class="obs-alert-error mt-4 rounded px-4 py-3">
						<p class="obs-alert-error-title text-sm font-semibold">Run update failed</p>
						<p class="obs-alert-error-body mt-1 font-mono text-xs">{modalError}</p>
					</div>
				{/if}

				{#if metadataDraft}
					<div class="mt-4 grid grid-cols-1 gap-3 text-sm sm:grid-cols-2">
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">Level</span>
							<Select
								class="w-full"
								placeholder="select level"
								bind:value={metadataDraft.level}
								options={levelOptions}
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
