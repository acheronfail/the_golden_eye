<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RunListStory from './RunListStory.svelte';
	import { runClips } from '../../fixtures';

	const virtualizedHistory = Array.from({ length: 116 }, (_, index) => {
		const seed = runClips[index % runClips.length];
		const timestamp = new Date(Date.UTC(2026, 6, 21, 12, 43, 9) - index * 3_600_000).toISOString();
		return {
			...seed,
			runId: `virtualized-run-${index + 1}`,
			path: seed.path ? `/runs/story/virtualized-run-${index + 1}.mp4` : '',
			fileName: seed.fileName ? `virtualized-run-${index + 1}.mp4` : '',
			metadata: { ...seed.metadata, timestamp }
		};
	});
	const firstInfinitePage = virtualizedHistory.slice(0, 50);

	const { Story } = defineMeta({
		title: 'Runs/Run list',
		component: RunListStory,
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story name="Loading folders" args={{ loading: true, clips: [], visibleClips: [], scannedDirectoryCount: 0 }} />
<Story name="Probing clips" args={{ loading: true, clips: [], visibleClips: [], scannedDirectoryCount: 2 }} />
<Story name="No folders configured" args={{ clips: [], visibleClips: [], directoryCount: 0 }} />
<Story name="No tagged clips" args={{ clips: [], visibleClips: [], directoryCount: 2 }} />
<Story name="Different runs" args={{ clips: runClips, visibleClips: runClips }} />
<Story name="Virtualized 116-run history" args={{ clips: virtualizedHistory, visibleClips: virtualizedHistory }} />
<Story
	name="Infinite history first page"
	args={{ clips: firstInfinitePage, visibleClips: firstInfinitePage, total: virtualizedHistory.length, hasMore: true }}
/>
<Story name="Scanning with results" args={{ loading: true, clips: runClips, visibleClips: runClips }} />
<Story name="No filter matches" args={{ clips: runClips, visibleClips: [], hasActiveFilters: true }} />
