<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import SessionStatistics from '$lib/features/statistics/SessionStatistics.svelte';
	import { sessionDetailFixture, sessionSummaryFixture } from './statisticsFixtures';
	const { Story } = defineMeta({
		title: 'Statistics/Sessions',
		component: SessionStatistics,
		args: {
			sessions: [sessionSummaryFixture],
			selectedSessionId: sessionSummaryFixture.sessionId,
			detail: sessionDetailFixture
		}
	});
</script>

<Story name="Completed session" />
<Story
	name="Single attempt"
	args={{
		sessions: [{ ...sessionSummaryFixture, counts: { total: 1, complete: 1, failed: 0, abort: 0, kia: 0 } }]
	}}
/>
<Story name="Empty" args={{ sessions: [], selectedSessionId: '', detail: null }} />
<Story name="Loading" args={{ detail: null, loading: true }} />
