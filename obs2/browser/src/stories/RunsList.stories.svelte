<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RunListStory from './RunListStory.svelte';
	import { runClips } from './fixtures';

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
<Story name="Scanning with results" args={{ loading: true, clips: runClips, visibleClips: runClips }} />
<Story name="No filter matches" args={{ clips: runClips, visibleClips: [], hasActiveFilters: true }} />
