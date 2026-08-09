<script lang="ts">
	import type { RunClip, RunSort } from '$lib/api';
	import RunList from '$lib/features/runs/RunList.svelte';
	import { createRunListRows } from '$lib/features/runs/runsView';

	let {
		loading = false,
		clips = [],
		visibleClips,
		total,
		generatedRunCount = 0,
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
		generatedRunCount?: number;
		hasMore?: boolean;
		scannedDirectoryCount?: number;
		directoryCount?: number | null;
		hasActiveFilters?: boolean;
		sort?: RunSort;
	} = $props();

	const storyClips = $derived.by(() => {
		if (generatedRunCount === 0 || clips.length === 0) return clips;
		return Array.from({ length: generatedRunCount }, (_, index) => {
			const seed = clips[index % clips.length];
			const timestamp = new Date(Date.UTC(2026, 6, 21, 12, 43, 9) - index * 1_000).toISOString();
			return {
				...seed,
				runId: `generated-run-${index + 1}`,
				path: seed.path ? `/runs/story/generated-run-${index + 1}.mp4` : '',
				fileName: seed.fileName ? `generated-run-${index + 1}.mp4` : '',
				metadata: { ...seed.metadata, timestamp }
			};
		});
	});
	const storyVisibleClips = $derived(visibleClips ?? storyClips);
	const storyTotal = $derived(total ?? storyClips.length);
	const storyRows = $derived(createRunListRows(storyVisibleClips, sort));
</script>

<div class="h-screen overflow-y-auto obs-content-scroller">
	<main class="mx-auto w-full max-w-3xl px-4 py-6 sm:px-6">
		<div class="mb-4">
			<h1 class="text-2xl font-semibold obs-heading">Runs</h1>
			<p class="mt-1 font-mono text-xs obs-dim">
				{storyVisibleClips.length} loaded of {storyTotal}{loading ? ' | scanning...' : ''}
			</p>
		</div>
		<RunList
			{loading}
			clips={storyClips}
			rows={storyRows}
			total={storyTotal}
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
</div>
