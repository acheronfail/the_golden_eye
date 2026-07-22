<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import FailedRunReviewDialog from '$lib/components/FailedRunReviewDialog.svelte';
	import { abortedRun, failedRun } from './fixtures';

	const { Story } = defineMeta({
		title: 'Runs/Failed run review',
		component: FailedRunReviewDialog,
		parameters: { layout: 'fullscreen' },
		args: {
			open: true,
			clips: [failedRun, abortedRun],
			close: () => {},
			keep: () => {},
			discard: () => {}
		}
	});
</script>

<Story name="Pending clips" />
<Story name="Loading" args={{ clips: [], loading: true }} />
<Story name="Action failed" args={{ error: 'The clip was moved before the review could be updated.' }} />
