<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import TimesStatistics from '$lib/features/statistics/TimesStatistics.svelte';
	import { completedRun, theEliteRun } from '../../fixtures';

	const runs = [
		{ ...completedRun, metadata: { ...completedRun.metadata, romVersion: 'pal' as const } },
		{
			...completedRun,
			runId: 'older-run',
			path: '',
			metadata: {
				...completedRun.metadata,
				timestamp: '2025-01-01T12:00:00Z',
				timeSeconds: 65,
				romVersion: 'ntsc-u' as const
			}
		},
		theEliteRun
	];
	const { Story } = defineMeta({
		title: 'Statistics/Times',
		component: TimesStatistics,
		args: { runs }
	});
</script>

<Story name="Best times" />
<Story name="History" args={{ level: 2, difficulty: '00 Agent' }} />
<Story name="No times" args={{ runs: [] }} />
<Story name="Empty history" args={{ runs: [], level: 1, difficulty: 'Agent' }} />
<Story name="Loading" args={{ loading: true }} />
<Story name="Error" args={{ error: 'The catalog could not be read.', retry: () => {} }} />

<Story name="More history" args={{ level: 2, difficulty: '00 Agent', loadMore: () => {} }} />
<Story name="Loading more history" args={{ level: 2, difficulty: '00 Agent', loadMore: () => {}, loadingMore: true }} />
<Story
	name="History page error"
	args={{ level: 2, difficulty: '00 Agent', error: 'Connection lost', retry: () => {} }}
/>
