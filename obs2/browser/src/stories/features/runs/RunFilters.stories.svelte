<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RunFiltersStory from './RunFiltersStory.svelte';

	const { Story } = defineMeta({
		title: 'Runs/Filters',
		component: RunFiltersStory,
		parameters: { layout: 'fullscreen' },
		args: { initialFilters: {}, initiallyCollapsed: false }
	});
</script>

<Story name="All runs" />
<Story name="Collapsed" args={{ initiallyCollapsed: true }} />
<Story name="Search" args={{ initialFilters: { search: 'facility 00 agent' } }} />
<Story name="Level and difficulty" args={{ initialFilters: { level: 'Facility', difficulty: '00 Agent' } }} />
<Story name="Status and language" args={{ initialFilters: { status: 'kia', language: 'jp' } }} />
<Story name="Time range" args={{ initialFilters: { minTime: '00:45', maxTime: '01:15' } }} />
<Story
	name="Collapsed with active filters"
	args={{ initialFilters: { search: 'control', status: 'failed', language: 'en' }, initiallyCollapsed: true }}
/>
<Story
	name="All filters active"
	args={{
		initialFilters: {
			search: 'speedrun',
			level: 'Control',
			difficulty: 'Agent',
			status: 'complete',
			language: 'en',
			minTime: '00:30',
			maxTime: '01:00'
		}
	}}
/>
