<script lang="ts">
	import type { LevelMatch } from '$lib/api';
	import MonitorHero from '$lib/features/monitor/MonitorHero.svelte';
	import MonitorWallClockTimers from '$lib/features/monitor/MonitorWallClockTimers.svelte';
	import type { MonitorWallClockSnapshot } from '$lib/features/monitor/monitorWallClocks.svelte';
	import { monitorPresentation } from '$lib/features/monitor/monitorView';

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
	const presentation = $derived(
		monitorPresentation({
			verified: true,
			monitoring: true,
			transition: null,
			recordingState: 'started',
			match,
			fps: null,
			showMonitorFps: false,
			onStop: () => {}
		})
	);
	const wallClocks = $derived<MonitorWallClockSnapshot>({
		sessionElapsedMs: 83_234,
		sessionRunning: timersRunning,
		levelElapsedMs: 12_345,
		levelRunning: timersRunning
	});
</script>

<div
	class="@container [container-type:size] flex h-full min-h-[36rem] items-center justify-center bg-(--obs-bg) p-8 text-(--obs-text) [--monitor-accent:var(--obs-success)] [--monitor-surface:var(--obs-success-surface)]"
>
	<div class={variant === 'mission-glass' ? 'grid w-[min(82cqw,42rem)] gap-4' : 'w-[min(82cqw,42rem)]'}>
		<MonitorWallClockTimers {variant} {wallClocks} />
		<MonitorHero {variant} verified={true} {presentation} {match} />
	</div>
</div>
