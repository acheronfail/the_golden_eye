<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorView from '$lib/features/monitor/MonitorView.svelte';
	import { monitorBaseArgs, monitorDesignArgs, monitorMatch as match } from './monitorStoryFixtures';

	const { Story } = defineMeta({
		title: 'Monitor/Monitor states/Healthy monitor FPS',
		component: MonitorView,
		parameters: { layout: 'fullscreen' },
		args: {
			...monitorBaseArgs,
			recordingState: 'started',
			match: match('start'),
			wallClockState: { ...monitorBaseArgs.wallClockState, levelTimerPhase: 'awaitingInitialBlack' },
			showMonitorFps: true,
			fps: { processedFps: 60, capturedFps: 60, sourceFps: 60, droppedFrames: 0, health: 'healthy' }
		}
	});
</script>

<Story name="Mission glass" args={monitorDesignArgs.missionGlass} />
<Story name="Signal band" args={monitorDesignArgs.signalBand} />
<Story name="For Your Eyes Only" args={monitorDesignArgs.debug} />

<Story
	name="Detailed fractional diagnostics"
	args={{
		...monitorDesignArgs.debug,
		match: {
			...match('start'),
			runtime_ms: 12.345678,
			match_regions: [{ label: 'time', x: 10, y: 20, w: 30, h: 40, score: 0.987654 }]
		},
		fps: { processedFps: 59.946789, capturedFps: 59.999999, sourceFps: 59.940059, droppedFrames: 0, health: 'healthy' },
		wallClockState: {
			...monitorBaseArgs.wallClockState,
			introSwirlDelayMs: 3654,
			fadeDetection: {
				detected: false,
				meanLuma: 12.345678,
				darkPixelPercent: 98.765432,
				sampleCount: 576,
				sampleRegion: { x: 107, y: 0, width: 640, height: 480 }
			}
		}
	}}
/>
