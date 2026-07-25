<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorView from '$lib/components/MonitorView.svelte';
	import { monitorBaseArgs, monitorDesignArgs, monitorMatch as match } from './monitorStoryFixtures';

	const { Story } = defineMeta({
		title: 'Monitor/Monitor states/Healthy monitor FPS',
		component: MonitorView,
		parameters: { layout: 'fullscreen' },
		args: {
			...monitorBaseArgs,
			recordingState: 'started',
			match: match('start'),
			showMonitorFps: true,
			fps: { processedFps: 60, capturedFps: 60, sourceFps: 60, droppedFrames: 0, health: 'healthy' }
		}
	});
</script>

<Story name="Mission glass" args={monitorDesignArgs.missionGlass} />
<Story name="Signal band" args={monitorDesignArgs.signalBand} />
<Story name="For Your Eyes Only" args={monitorDesignArgs.debug} />
