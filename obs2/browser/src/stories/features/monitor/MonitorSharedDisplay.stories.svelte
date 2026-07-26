<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorSharedDisplayStory from './MonitorSharedDisplayStory.svelte';

	const { Story } = defineMeta({
		title: 'Monitor/Shared display',
		component: MonitorSharedDisplayStory,
		parameters: { layout: 'fullscreen' },
		args: { variant: 'mission-glass', timesAvailable: true, timersRunning: true }
	});
</script>

<Story name="Mission glass" />
<Story name="Mission glass placeholders" args={{ timesAvailable: false, timersRunning: false }} />
<Story name="Signal band" args={{ variant: 'signal-band' }} />
<Story name="Signal band placeholders" args={{ variant: 'signal-band', timesAvailable: false, timersRunning: false }} />
