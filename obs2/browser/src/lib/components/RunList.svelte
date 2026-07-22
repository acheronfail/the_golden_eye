<script lang="ts">
	import type { RunClip } from '$lib/api';
	import RunListItem from '$lib/components/RunListItem.svelte';

	let {
		loading,
		clips,
		visibleClips,
		scannedDirectoryCount,
		directoryCount,
		hasActiveFilters,
		clearFilters,
		busyPath = null,
		fileBrowserLabel,
		open,
		rename,
		reveal,
		remove
	}: {
		loading: boolean;
		clips: RunClip[];
		visibleClips: RunClip[];
		scannedDirectoryCount: number;
		directoryCount: number | null;
		hasActiveFilters: boolean;
		clearFilters: () => void;
		busyPath?: string | null;
		fileBrowserLabel: string;
		open: (clip: RunClip) => void;
		rename: (clip: RunClip) => void | Promise<void>;
		reveal: (clip: RunClip) => void | Promise<void>;
		remove: (clip: RunClip) => void | Promise<void>;
	} = $props();

	let openMenuPath = $state<string | null>(null);

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
	<ul class="flex flex-col gap-1.5">
		{#each visibleClips as clip (clip.path)}
			<li>
				<RunListItem
					{clip}
					busy={busyPath === clip.path}
					menuOpen={openMenuPath === clip.path}
					onMenuOpenChange={(isOpen) => setMenuOpen(clip.path, isOpen)}
					{fileBrowserLabel}
					{open}
					{rename}
					{reveal}
					{remove}
				/>
			</li>
		{/each}
	</ul>
{/if}
