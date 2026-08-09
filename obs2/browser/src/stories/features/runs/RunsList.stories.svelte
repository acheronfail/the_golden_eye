<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RunListStory from './RunListStory.svelte';
	import { runClips } from '../../fixtures';

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
<Story name="Virtualized 116-run history" args={{ clips: runClips, generatedRunCount: 116 }} />
<Story name="Virtualized 5,000-run same-date history" args={{ clips: runClips, generatedRunCount: 5000 }} />
<Story
	name="Infinite 10,000-run history first page"
	args={{ clips: runClips, generatedRunCount: 50, total: 10000, hasMore: true }}
/>
<Story name="Scanning with results" args={{ loading: true, clips: runClips, visibleClips: runClips }} />
<Story name="No filter matches" args={{ clips: [], visibleClips: [], total: 0, hasActiveFilters: true }} />
