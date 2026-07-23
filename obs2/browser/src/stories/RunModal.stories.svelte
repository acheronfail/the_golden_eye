<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RunDetailStory from './RunDetailStory.svelte';
	import { completedRun, failedRun, pendingRun, untaggedRun, youtubeStatus } from './fixtures';

	const { Story } = defineMeta({
		title: 'Runs/Run modal',
		component: RunDetailStory,
		parameters: { layout: 'fullscreen' },
		args: { clip: completedRun, status: youtubeStatus({ enabled: false }) }
	});
</script>

<Story name="Completed run" />
<Story name="Pending cleanup" args={{ clip: pendingRun }} />
<Story name="Failed run" args={{ clip: failedRun }} />
<Story name="Missing metadata" args={{ clip: untaggedRun }} />
<Story name="Saving metadata" args={{ modalBusy: 'metadata' }} />
<Story name="Deleting" args={{ modalBusy: 'delete' }} />
<Story name="Update failed" args={{ modalError: 'The clip was moved or renamed outside the plugin.' }} />
