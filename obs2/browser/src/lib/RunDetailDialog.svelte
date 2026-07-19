<script lang="ts">
	import { linear } from 'svelte/easing';
	import { tweened } from 'svelte/motion';
	import {
		backend,
		type EditableRunMetadata,
		type RunClip,
		type YouTubeUploadHistoryEntry,
		type YouTubeUploadStatus
	} from '$lib/api';
	import { Select, type SelectOption } from '$lib';
	import { DIFFICULTY_OPTIONS, LANGUAGE_OPTIONS, STATUS_OPTIONS, fileRows, runDetail } from '$lib/runsView';

	let {
		clip,
		metadataDraft = $bindable(),
		metadataDirty,
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
		youtubeConnecting,
		youtubeUpload,
		youtubeHistory,
		youtubeError,
		connectYouTube,
		uploadToYouTube,
		forgetYouTubeUpload
	}: {
		clip: RunClip | null;
		metadataDraft: EditableRunMetadata | null;
		metadataDirty: boolean;
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
		youtubeConnecting: boolean;
		youtubeUpload: YouTubeUploadStatus | null;
		youtubeHistory: YouTubeUploadHistoryEntry | null;
		youtubeError: string | null;
		connectYouTube: () => void;
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
		if (!youtubeConnected) return youtubeConnecting ? 'Connecting...' : 'Connect YouTube';
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
			youtubeConnecting ||
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
	let visibleYoutubeUploadError = $derived(
		youtubeUpload?.state === 'failed' && youtubeUpload.id !== youtubeDismissedUploadErrorId
			? (youtubeUpload.error ?? 'Upload failed')
			: null
	);
	let visibleYoutubeError = $derived(youtubeError && youtubeError !== youtubeDismissedStoreError ? youtubeError : null);

	const youtubeClick = () => {
		if (youtubeConnected) {
			uploadToYouTube();
		} else {
			connectYouTube();
		}
	};
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
			youtubeHelpOpen = !youtubeIsUploaded;
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
							<h3 class="font-mono text-xs font-semibold tracking-[0.2em] text-[var(--obs-text-muted)] uppercase">
								YouTube
							</h3>
							<button
								type="button"
								class="obs-text-button obs-button-xs cursor-pointer rounded-full"
								aria-expanded={youtubeHelpOpen}
								aria-label="Explain YouTube upload history"
								onclick={() => (youtubeHelpOpen = !youtubeHelpOpen)}>?</button
							>
						</div>
						{#if youtubeHelpOpen}
							<div class="mb-3 grid gap-2 text-xs leading-relaxed text-[var(--obs-text-muted)]">
								{#if youtubeIsUploaded}
									<p>
										The plugin remembers videos it uploaded from this computer and links them to the clip's file path.
										If a clip is moved, renamed, or edited with other video apps, it may not recognise it as the same
										video and might not remember that it was uploaded or not.
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
								{:else}
									<p>
										Upload this clip to YouTube using the title, description, and visibility templates configured in
										<a class="obs-text-button obs-button-xs" href="/options?tab=youtube">Options → YouTube</a>.
									</p>
								{/if}
							</div>
						{/if}
						<div class="flex flex-col items-center gap-2 text-center">
							{#if !youtubeIsUploaded}
								<div class="flex flex-wrap items-center justify-center gap-2">
									<button
										type="button"
										onclick={youtubeClick}
										disabled={youtubeButtonDisabled}
										class="obs-button obs-button-gold px-3 py-1.5 font-mono text-sm disabled:cursor-not-allowed disabled:opacity-50"
									>
										{youtubeButtonLabel}
									</button>
								</div>
							{/if}
							{#if youtubeOpenUrl}
								<div class="flex w-full items-center justify-center gap-2 px-2 sm:px-8">
									<input
										class="obs-input min-w-0 flex-1 truncate border-[var(--obs-border-strong)] px-3 py-1.5 text-center font-mono text-xs shadow-[inset_0_1px_0_var(--obs-border-soft)]"
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
										class="h-full bg-[var(--obs-gold)]"
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
										class="obs-alert-error-body mt-1 font-mono text-[11px] leading-relaxed break-words whitespace-pre-wrap"
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
										class="obs-alert-error-body mt-1 font-mono text-[11px] leading-relaxed break-words whitespace-pre-wrap"
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
							/>
						</label>
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">ROM language</span>
							<Select
								class="w-full"
								placeholder="select language"
								bind:value={metadataDraft.romLanguage}
								options={LANGUAGE_OPTIONS}
							/>
						</label>
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">Time</span>
							<input
								class="obs-input px-3 py-2 font-mono"
								bind:value={metadataDraft.time}
								onblur={normalizeDraftTime}
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
							/>
						</label>
						<label class="flex min-w-0 flex-col gap-1">
							<span class="obs-dim font-mono text-xs">Status</span>
							<Select
								class="w-full"
								placeholder="select status"
								bind:value={metadataDraft.status}
								options={STATUS_OPTIONS}
							/>
						</label>
					</div>

					<div class="mt-4 flex justify-end">
						<button
							type="button"
							onclick={saveMetadata}
							disabled={modalBusy !== null || !metadataDirty}
							class="obs-button obs-button-gold px-3 py-2 font-mono text-xs"
						>
							{modalBusy === 'metadata' ? 'saving...' : 'save metadata'}
						</button>
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
