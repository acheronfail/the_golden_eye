<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import OptionsPageStory from './OptionsPageStory.svelte';

	const { Story } = defineMeta({
		title: 'Options/Full page',
		component: OptionsPageStory,
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story name="General in OBS dock" />
<Story name="Recording in OBS dock" args={{ section: 'recording' }} />
