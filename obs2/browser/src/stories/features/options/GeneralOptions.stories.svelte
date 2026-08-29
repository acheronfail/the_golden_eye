<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import GeneralOptionsStory from './GeneralOptionsStory.svelte';

	const { Story } = defineMeta({
		title: 'Options/General',
		component: GeneralOptionsStory,
		parameters: { layout: 'fullscreen' },
		args: { autoStartMonitorOnLaunch: false }
	});
</script>

<Story name="Default" />
<Story name="Auto-start enabled" args={{ autoStartMonitorOnLaunch: true }} />
