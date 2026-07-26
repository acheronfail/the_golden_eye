<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import StatisticsDashboard from '$lib/features/statistics/StatisticsDashboard.svelte';
	import { sessionDetailFixture, sessionSummaryFixture, statisticsFixture } from './statisticsFixtures';
	const { Story } = defineMeta({
		title: 'Statistics/Dashboard',
		component: StatisticsDashboard,
		args: {
			data: statisticsFixture,
			loading: false,
			error: null,
			levelNumber: 1,
			difficultyNumber: 0,
			tab: 'overview',
			sessions: [sessionSummaryFixture],
			selectedSessionId: sessionSummaryFixture.sessionId,
			sessionDetail: sessionDetailFixture
		}
	});
</script>

<Story name="Overview" />
<Story name="Improvement" args={{ tab: 'improvement' }} />
<Story name="Outcomes" args={{ tab: 'outcomes' }} />
<Story name="Sessions" args={{ tab: 'sessions' }} />
<Story name="Loading" args={{ data: null, loading: true }} />
<Story name="Error" args={{ error: 'The run catalog could not be loaded.' }} />
