<script lang="ts">
	import type { RunClip, RunSort } from '$lib/api';
	import RunListItem from '$lib/components/RunListItem.svelte';
	import RunSortMenu from '$lib/components/RunSortMenu.svelte';
	import SectionTitle from '$lib/components/SectionTitle.svelte';
	import { groupRunClips } from '$lib/utils/runsView';

	let {
		loading,
		clips,
		visibleClips,
		scannedDirectoryCount,
		directoryCount,
		hasActiveFilters,
		clearFilters,
		sort,
		onSortChange,
		busyPath = null,
		fileBrowserLabel,
		open,
		rename,
		reveal,
		remove,
		keep = () => {}
	}: {
		loading: boolean;
		clips: RunClip[];
		visibleClips: RunClip[];
		scannedDirectoryCount: number;
		directoryCount: number | null;
		hasActiveFilters: boolean;
		clearFilters: () => void;
		sort: RunSort;
		onSortChange: (sort: RunSort) => void;
		busyPath?: string | null;
		fileBrowserLabel: string;
		open: (clip: RunClip) => void;
		rename: (clip: RunClip) => void | Promise<void>;
		reveal: (clip: RunClip) => void | Promise<void>;
		remove: (clip: RunClip) => void | Promise<void>;
		keep?: (clip: RunClip) => void | Promise<void>;
	} = $props();

	let openMenuPath = $state<string | null>(null);
	const groups = $derived(groupRunClips(visibleClips, sort));
	const showDate = $derived(sort === 'fastest' || sort === 'slowest');

	function setMenuOpen(path: string, open: boolean) {
		if (open) {
			openMenuPath = path;
		} else if (openMenuPath === path) {
			openMenuPath = null;
		}
	}
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
	<div class="flex items-center justify-between border-b-2 border-(--obs-border-muted) pb-1">
		<p class="font-mono text-xs"><strong>{visibleClips.length}</strong> {visibleClips.length === 1 ? 'run' : 'runs'}</p>
		<RunSortMenu {sort} onChange={onSortChange} />
	</div>

	<div role="list" aria-label="Runs">
		{#each groups as group (group.label ?? sort)}
			<section aria-label={group.label ?? undefined} class:mt-3={group.label !== null}>
				{#if group.label}
					<SectionTitle
						title={group.label}
						detail={`${group.clips.length} ${group.clips.length === 1 ? 'run' : 'runs'}`}
						class="mb-0.5"
					/>
				{/if}
				{#each group.clips as clip (clip.runId ?? clip.path)}
					<div role="listitem">
						<RunListItem
							{clip}
							{showDate}
							busy={busyPath === (clip.runId ?? clip.path)}
							menuOpen={openMenuPath === (clip.runId ?? clip.path)}
							onMenuOpenChange={(isOpen) => setMenuOpen(clip.runId ?? clip.path, isOpen)}
							{fileBrowserLabel}
							{open}
							{rename}
							{reveal}
							{remove}
							{keep}
						/>
					</div>
				{/each}
			</section>
		{/each}
	</div>
{/if}
