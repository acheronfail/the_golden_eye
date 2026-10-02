<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import ReadmePreview from './ReadmePreview.svelte';

	const { Story } = defineMeta({
		title: 'App/README preview',
		component: ReadmePreview,
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story name="Mission glass" />
<Story name="Signal band" args={{ design: 'signal-band' }} />
<Story name="Runs" args={{ view: 'runs' }} />
<Story name="Statistics" args={{ view: 'statistics' }} />
