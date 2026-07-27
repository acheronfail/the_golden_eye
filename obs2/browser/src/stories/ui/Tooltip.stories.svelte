<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import TooltipStory from './TooltipStory.svelte';

	const { Story } = defineMeta({
		title: 'UI/Tooltip',
		component: TooltipStory,
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story name="Default" />
