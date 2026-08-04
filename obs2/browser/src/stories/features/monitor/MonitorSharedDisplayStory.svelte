<script lang="ts">
	import type { LevelMatch } from '$lib/api';
	import MonitorView from '$lib/features/monitor/MonitorView.svelte';

	let {
		variant,
		timesAvailable = true,
		timersRunning = true
	}: {
		variant: 'mission-glass' | 'signal-band';
		timesAvailable?: boolean;
		timersRunning?: boolean;
	} = $props();

	const match = $derived<LevelMatch>({
		screen: timesAvailable ? 'stats' : 'unknown',
		mission: 2,
		part: 1,
		difficulty: 0,
		detected_lang: 'en',
		times: timesAvailable ? { time: 58, target_time: 65, best_time: 61 } : null,
		runtime_ms: 8.4
	});
	const wallClockState = $derived({
		sessionStartedAtUnixMs: timersRunning ? Date.now() - 83_234 : null,
		sessionElapsedMs: 83_234,
		sessionRunning: timersRunning,
		levelStartedAtUnixMs: timersRunning ? Date.now() - 12_345 : null,
		levelElapsedMs: 12_345,
		levelRunning: timersRunning,
		levelPaused: false,
		levelStartReason: timersRunning ? ('fade' as const) : null,
		levelTimerPhase: timersRunning ? ('running' as const) : ('idle' as const),
		introSwirlDelayMs: timersRunning ? 3_650 : null,
		fadeDetection: timersRunning
			? {
					detected: false,
					meanLuma: 84,
					darkPixelPercent: 12,
					sampleCount: 576,
					sampleRegion: { x: 107, y: 0, width: 640, height: 480 }
				}
			: null
	});
</script>

<MonitorView
	design={variant}
	verified={true}
	monitoring={true}
	recordingState="started"
	{match}
	{wallClockState}
	onStop={() => {}}
/>
