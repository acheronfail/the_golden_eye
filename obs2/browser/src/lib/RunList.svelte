<script lang="ts">
	import type { RunClip } from '$lib/api';
	import { isCompleted, formatDate, runMetaChips } from '$lib/runsView';

	let {
		loading,
		clips,
		visibleClips,
		scannedDirectoryCount,
		directoryCount,
		hasActiveFilters,
		clearFilters,
		select
	}: {
		loading: boolean;
		clips: RunClip[];
		visibleClips: RunClip[];
		scannedDirectoryCount: number;
		directoryCount: number | null;
		hasActiveFilters: boolean;
		clearFilters: () => void;
		select: (clip: RunClip) => void;
	} = $props();
</script>

{#if loading && clips.length === 0}
	<p class="obs-dim font-mono text-sm">
		{scannedDirectoryCount === 0 ? 'Searching run folders...' : 'Probing clips...'}
	</p>
{:else if directoryCount === 0}
	<div class="obs-empty-state rounded px-4 py-6 text-center">
		<p class="obs-muted text-sm">No run folders configured.</p>
		<p class="obs-dim mt-1 font-mono text-xs">Set completed and failed output folders in Options.</p>
	</div>
{:else if clips.length === 0}
	<div class="obs-empty-state rounded px-4 py-6 text-center">
		<p class="obs-muted text-sm">No tagged clips found.</p>
		<p class="obs-dim mt-1 font-mono text-xs">New clips saved by this plugin will appear here.</p>
	</div>
{:else if visibleClips.length === 0}
	<div class="obs-empty-state rounded px-4 py-6 text-center">
		<p class="obs-muted text-sm">No runs match the current filters.</p>
		<button
			type="button"
			class="obs-text-button mt-3 px-2 py-1 font-mono text-xs"
			disabled={!hasActiveFilters}
			onclick={clearFilters}
		>
			clear filters
		</button>
	</div>
{:else}
	{#if loading}
		<p class="obs-dim mb-3 font-mono text-xs">Search still running...</p>
	{/if}
	<ul class="flex flex-col gap-1.5">
		{#each visibleClips as clip (clip.path)}
			<li>
				<button
					type="button"
					class="obs-list-button group grid min-h-16 w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-2 rounded px-3 py-2 text-left transition-colors"
					class:obs-list-button-success={isCompleted(clip)}
					onclick={() => select(clip)}
				>
					<span class="flex min-w-0 flex-col gap-1">
						<span class="obs-list-title min-w-0 truncate text-sm font-semibold" title={clip.fileName}
							>{clip.fileName}</span
						>
						<span class="flex min-w-0 flex-wrap gap-1">
							{#each runMetaChips(clip) as chip}
								<span class="{chip.class} rounded border px-1.5 py-0.5 font-mono text-[10px] leading-tight"
									>{chip.label}</span
								>
							{/each}
						</span>
						<span
							class="obs-list-detail min-w-0 truncate font-mono text-[10px]"
							title={formatDate(clip.metadata.timestamp)}
						>
							{formatDate(clip.metadata.timestamp)}
						</span>
					</span>
					<span
						class="obs-list-arrow shrink-0 font-mono transition-transform group-hover:translate-x-1"
						aria-hidden="true">→</span
					>
				</button>
			</li>
		{/each}
	</ul>
{/if}
