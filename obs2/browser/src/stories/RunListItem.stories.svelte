<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RunListItemStory from './RunListItemStory.svelte';
	import { completedRun, failedRun, untaggedRun } from './fixtures';

	const { Story } = defineMeta({
		title: 'Runs/Run list item',
		component: RunListItemStory,
		parameters: { layout: 'fullscreen' },
		args: { clip: completedRun, busy: false, menuOpen: false }
	});
</script>

<Story name="Completed" />
<Story name="Failed" args={{ clip: failedRun }} />
<Story name="Untagged" args={{ clip: untaggedRun }} />
<Story name="Actions open" args={{ menuOpen: true }} />
<Story name="Busy" args={{ busy: true }} />
