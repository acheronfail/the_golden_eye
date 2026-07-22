<script lang="ts">
	import { onDestroy } from 'svelte';
	import { backend, type EditableRunMetadata, type RunClip } from '$lib/api';
	import RunYouTubeSection from '$lib/components/RunYouTubeSection.svelte';
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

				<RunYouTubeSection {clip} />

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
