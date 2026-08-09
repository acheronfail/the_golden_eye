<script lang="ts">
	import type { RunClip, RunSort } from '$lib/api';
	import RunList from '$lib/features/runs/RunList.svelte';

	let {
		loading = false,
		clips = [],
		visibleClips = clips,
		total = clips.length,
		hasMore = false,
		scannedDirectoryCount = 2,
		directoryCount = 2,
		hasActiveFilters = false,
		sort = 'newest'
	}: {
		loading?: boolean;
		clips?: RunClip[];
		visibleClips?: RunClip[];
		total?: number;
		hasMore?: boolean;
		scannedDirectoryCount?: number;
		directoryCount?: number | null;
		hasActiveFilters?: boolean;
		sort?: RunSort;
	} = $props();
</script>

<main class="mx-auto w-full max-w-3xl px-4 py-6 sm:px-6">
	<div class="mb-4">
		<h1 class="text-2xl font-semibold obs-heading">Runs</h1>
		<p class="mt-1 font-mono text-xs obs-dim">
			{visibleClips.length} loaded of {total}{loading ? ' | scanning...' : ''}
		</p>
	</div>
	<RunList
		{loading}
		{clips}
		{visibleClips}
		{total}
		{hasMore}
		loadMore={() => {}}
		{scannedDirectoryCount}
		{directoryCount}
		{hasActiveFilters}
		{sort}
		onSortChange={() => {}}
		fileBrowserLabel="Show in file browser"
		clearFilters={() => {}}
		open={() => {}}
		rename={() => {}}
		reveal={() => {}}
		remove={() => {}}
	/>
</main>
