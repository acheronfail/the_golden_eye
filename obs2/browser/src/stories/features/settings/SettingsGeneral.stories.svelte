<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SettingsGeneralStory from './SettingsGeneralStory.svelte';

	const { Story } = defineMeta({
		title: 'Options/General',
		component: SettingsGeneralStory,
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story name="Stable releases only" />
<Story name="Include pre-releases" args={{ includePrereleases: true }} />
