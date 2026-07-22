<script lang="ts">
	import { backend, type RunClip } from '$lib/api';
	import { statusLabel } from '$lib/utils/runsView';

	let {
		open,
		clips,
		loading = false,
		busy = false,
		error = null,
		close,
		keep,
		discard
	}: {
		open: boolean;
		clips: RunClip[];
		loading?: boolean;
		busy?: boolean;
		error?: string | null;
		close: () => void;
		keep: (paths: string[]) => void;
		discard: (paths: string[]) => void;
	} = $props();

	let selected = $state<string[]>([]);
	const selectedSet = $derived(new Set(selected));
	const allSelected = $derived(clips.length > 0 && clips.every((clip) => selectedSet.has(clip.path)));

	const toggle = (path: string) => {
		selected = selectedSet.has(path) ? selected.filter((candidate) => candidate !== path) : [...selected, path];
	};

	const toggleAll = () => {
		selected = allSelected ? [] : clips.map((clip) => clip.path);
	};

	const discardSelected = () => {
		if (selected.length === 0) return;
		if (!confirm(`Permanently discard ${selected.length} selected failed clip${selected.length === 1 ? '' : 's'}?`))
			return;
		const paths = selected;
		selected = [];
		discard(paths);
	};

	const keepSelected = () => {
		const paths = selected;
		selected = [];
		keep(paths);
	};
</script>

{#if open}
	<div class="obs-overlay fixed inset-0 z-60 flex items-center justify-center p-4">
		<button type="button" aria-label="Review failed runs later" class="absolute inset-0 cursor-default" onclick={close}
		></button>
		<dialog
			open
			aria-label="Review failed runs"
			class="obs-dialog relative z-10 m-0 max-h-full w-full max-w-4xl overflow-hidden rounded p-0"
		>
			<header class="obs-dialog-header px-4 py-3">
				<h2 class="obs-heading text-lg font-semibold">Review failed runs</h2>
				<p class="obs-dim mt-1 font-mono text-xs">
					Keep interesting attempts or explicitly discard the ones you do not need.
				</p>
			</header>

			<div class="max-h-[calc(100vh-10rem)] overflow-y-auto p-4">
				{#if error}
					<div class="obs-alert-error mb-4 rounded px-4 py-3">
						<p class="obs-alert-error-title text-sm font-semibold">Could not update failed runs</p>
						<p class="obs-alert-error-body mt-1 font-mono text-xs">{error}</p>
					</div>
				{/if}

				{#if loading && clips.length === 0}
					<p class="obs-dim py-8 text-center font-mono text-sm">Loading failed runs...</p>
				{:else}
					<label class="mb-3 flex items-center gap-3 font-mono text-xs">
						<input
							type="checkbox"
							checked={allSelected}
							onchange={toggleAll}
							disabled={busy || clips.length === 0}
							class="obs-checkbox rounded"
						/>
						Select all ({clips.length})
					</label>

					<div class="grid gap-3">
						{#each clips as clip (clip.path)}
							<article
								class="grid gap-3 rounded border border-(--obs-border) p-3 sm:grid-cols-[auto_14rem_minmax(0,1fr)] sm:items-center"
							>
								<input
									type="checkbox"
									aria-label={`Select ${clip.fileName}`}
									checked={selectedSet.has(clip.path)}
									onchange={() => toggle(clip.path)}
									disabled={busy}
									class="obs-checkbox rounded"
								/>
								<!-- svelte-ignore a11y_media_has_caption -->
								<video
									src={backend.runVideoUrl(clip.path)}
									controls
									preload="metadata"
									class="obs-preview aspect-video w-full"
								></video>
								<div class="min-w-0">
									<p class="obs-heading truncate text-sm font-semibold" title={clip.fileName}>{clip.fileName}</p>
									<p class="obs-dim mt-1 font-mono text-xs">
										{clip.metadata.level || 'unknown'} · {clip.metadata.difficulty || 'unknown'} · {statusLabel(
											clip.metadata.status
										)}
									</p>
									{#if clip.metadata.time}<p class="obs-muted mt-1 font-mono text-xs">{clip.metadata.time}</p>{/if}
								</div>
							</article>
						{/each}
					</div>
				{/if}
			</div>

			<footer class="obs-dialog-header flex flex-wrap justify-end gap-2 px-4 py-3">
				<button type="button" onclick={close} disabled={busy} class="obs-text-button px-3 py-2 font-mono text-xs"
					>Review later</button
				>
				<button
					type="button"
					onclick={keepSelected}
					disabled={busy || selected.length === 0}
					class="obs-text-button px-3 py-2 font-mono text-xs"
				>
					{busy ? 'working...' : `Keep selected (${selected.length})`}
				</button>
				<button
					type="button"
					onclick={discardSelected}
					disabled={busy || selected.length === 0}
					class="obs-text-button obs-button-danger px-3 py-2 font-mono text-xs"
				>
					Discard selected ({selected.length})
				</button>
			</footer>
		</dialog>
	</div>
{/if}
