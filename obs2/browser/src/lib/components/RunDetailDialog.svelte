<script lang="ts">
	import { onDestroy } from 'svelte';
	import { backend, type EditableRunMetadata, type RunClip } from '$lib/api';
	import RunYouTubeSection from '$lib/components/RunYouTubeSection.svelte';
	import SectionTitle from '$lib/components/SectionTitle.svelte';
	import Select from '$lib/components/Select.svelte';
	import {
		DIFFICULTY_OPTIONS,
		LANGUAGE_OPTIONS,
		STATUS_OPTIONS,
		fileRows,
		runDetail,
		type RunDetailView
	} from '$lib/utils/runsView';

	let {
		clip,
		metadataDraft = $bindable(),
		view
	}: {
		clip: RunClip | null;
		metadataDraft: EditableRunMetadata | null;
		view: RunDetailView;
	} = $props();

	let metadataSaveTimer: ReturnType<typeof setTimeout> | null = null;

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
			<header class="obs-dialog-header flex items-start gap-3 px-4 py-3">
				<div class="min-w-0 flex-1">
					<h2 class="obs-heading truncate text-lg font-semibold" title={clip.fileName || `${clip.metadata.level} run`}>
						{clip.fileName || `${clip.metadata.level} run history`}
					</h2>
					<p class="obs-dim mt-1 truncate font-mono text-xs" title={runDetail(clip)}>{runDetail(clip)}</p>
				</div>
				<button
					type="button"
					class="obs-text-button shrink-0 px-1.5 py-0.5 text-xs"
					aria-label="Close run viewer"
					onclick={view.actions.close}
				>
					x
				</button>
			</header>

			<div class="max-h-[calc(100vh-9rem)] overflow-y-auto p-4">
				{#if clip.path && clip.retentionState === 'pending'}
					<section
						aria-label="Pending video retention"
						class="obs-alert-warning mb-4 flex flex-col gap-3 rounded px-4 py-3 sm:flex-row sm:items-center"
					>
						<div class="min-w-0 flex-1">
							<p class="obs-alert-warning-title text-sm font-semibold">Pending cleanup</p>
							<p class="obs-alert-warning-body mt-1 text-xs">
								This video is temporary and will be deleted when it falls outside your recent-run history. Keep it to
								retain the video.
							</p>
						</div>
						<button
							type="button"
							onclick={view.actions.keep}
							disabled={view.modal.busy !== null}
							class="obs-text-button obs-button-gold shrink-0 px-3 py-2 font-mono text-xs"
						>
							{view.modal.busy === 'keep' ? 'keeping...' : 'keep video'}
						</button>
					</section>
				{/if}

				<div class="mb-4 flex flex-wrap justify-end gap-2">
					<button
						type="button"
						onclick={view.actions.delete}
						disabled={view.modal.busy !== null}
						class="obs-text-button obs-button-danger px-2 py-1 font-mono text-xs">delete</button
					>
					{#if clip.path}
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
					{/if}
				</div>
				{#if clip.path}
					<!-- svelte-ignore a11y_media_has_caption -->
					<video src={backend.runVideoUrl(clip.path)} controls class="obs-preview aspect-video w-full"></video>
					<RunYouTubeSection {clip} />
				{:else}
					<p class="obs-empty-state rounded px-4 py-6 text-center text-sm">
						The video has been removed. Run history is still available.
					</p>
				{/if}

				{#if view.modal.error}
					<div class="obs-alert-error mt-4 rounded px-4 py-3">
						<p class="obs-alert-error-title text-sm font-semibold">Run update failed</p>
						<p class="obs-alert-error-body mt-1 font-mono text-xs">{view.modal.error}</p>
					</div>
				{/if}

				{#if metadataDraft}
					<section class="mt-4">
						<SectionTitle title="Metadata" class="mb-3" />
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
