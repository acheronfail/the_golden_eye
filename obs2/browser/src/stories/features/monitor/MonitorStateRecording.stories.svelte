<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorView from '$lib/features/monitor/MonitorView.svelte';
	import { readmeRuns } from '../../readmeFixtures';
	import { monitorBaseArgs, monitorDesignArgs, monitorMatch as match } from './monitorStoryFixtures';

	const readmeArgs = {
		match: { ...match('start'), mission: 1, part: 2, difficulty: 2 },
		wallClockState: {
			...monitorBaseArgs.wallClockState,
			sessionElapsedMs: 1_345_000,
			levelTimerPhase: 'awaitingInitialBlack' as const
		},
		recentRuns: readmeRuns.slice(0, 5)
	};

	const { Story } = defineMeta({
		title: 'Monitor/Monitor states/Recording',
		component: MonitorView,
		parameters: { layout: 'fullscreen' },
		args: {
			...monitorBaseArgs,
			recordingState: 'started',
			match: match('start'),
			wallClockState: { ...monitorBaseArgs.wallClockState, levelTimerPhase: 'awaitingInitialBlack' }
		}
	});
</script>

<Story name="Mission glass" args={monitorDesignArgs.missionGlass} />
<Story name="Signal band" args={monitorDesignArgs.signalBand} />
<Story name="For Your Eyes Only" args={monitorDesignArgs.debug} />

<Story name="README preview" args={{ ...monitorDesignArgs.missionGlass, ...readmeArgs }} />
<Story name="README signal band" args={{ ...monitorDesignArgs.signalBand, ...readmeArgs }} />
