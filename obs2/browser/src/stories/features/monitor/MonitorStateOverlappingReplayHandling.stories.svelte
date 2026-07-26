<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorView from '$lib/features/monitor/MonitorView.svelte';
	import { monitorBaseArgs, monitorDesignArgs, monitorMatch as match } from './monitorStoryFixtures';

	// Replay-buffer handling only renders in the debug view, so this state has a
	// single story.
	const { Story } = defineMeta({
		title: 'Monitor/Monitor states/Overlapping replay handling',
		component: MonitorView,
		parameters: { layout: 'fullscreen' },
		args: {
			...monitorBaseArgs,
			recordingState: 'started',
			match: match('start'),
			replaySaves: [
				{
					trackingId: 43,
					saveId: 9,
					stage: 'scheduled',
					level: 'Facility',
					difficulty: '00 Agent',
					runStatus: 'complete',
					estimatedDurationSecs: 68
				},
				{
					trackingId: 42,
					saveId: 8,
					stage: 'trimming',
					level: 'Dam',
					difficulty: 'Agent',
					runStatus: 'failed',
					estimatedDurationSecs: 82
				},
				{
					trackingId: 41,
					saveId: 7,
					stage: 'failed',
					level: 'Runway',
					difficulty: 'Secret Agent',
					runStatus: 'complete',
					estimatedDurationSecs: 51,
					error: 'OBS replay buffer save timed out'
				}
			]
		}
	});
</script>

<Story name="For Your Eyes Only" args={monitorDesignArgs.debug} />
