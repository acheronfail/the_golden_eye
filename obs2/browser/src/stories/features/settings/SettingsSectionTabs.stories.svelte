<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SettingsSectionTabsStory from './SettingsSectionTabsStory.svelte';

	const { Story } = defineMeta({
		title: 'Options/Section selector',
		component: SettingsSectionTabsStory,
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story name="General selected" />
<Story name="Recording selected" args={{ initialSection: 'recording' }} />
<Story name="Notifications selected" args={{ initialSection: 'notifications' }} />
<Story name="YouTube available" args={{ initialSection: 'youtube', includeYouTube: true }} />
