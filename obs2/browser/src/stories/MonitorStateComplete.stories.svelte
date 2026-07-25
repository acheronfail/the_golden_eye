<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorView from '$lib/components/MonitorView.svelte';
	import { monitorBaseArgs, monitorDesignArgs, monitorMatch as match } from './monitorStoryFixtures';

	const { Story } = defineMeta({
		title: 'Monitor/Monitor states/Complete',
		component: MonitorView,
		parameters: { layout: 'fullscreen' },
		args: {
			...monitorBaseArgs,
			recordingState: 'complete',
			match: match('stats', { time: 58, target_time: 65, best_time: 61 })
		}
	});
</script>

<Story name="Mission glass" args={monitorDesignArgs.missionGlass} />
<Story name="Signal band" args={monitorDesignArgs.signalBand} />
<Story name="For Your Eyes Only" args={monitorDesignArgs.debug} />
