<script lang="ts">
	import { formatWallClockTime, type MonitorWallClockSnapshot } from './monitorWallClocks.svelte';

	let {
		wallClocks,
		containerClass = '',
		inactiveTimerClass = '',
		activeLabelClass = '',
		levelLabelClass = '',
		role
	}: {
		wallClocks: MonitorWallClockSnapshot;
		containerClass?: string;
		inactiveTimerClass?: string;
		activeLabelClass?: string;
		levelLabelClass?: string;
		role?: 'group';
	} = $props();

	const levelTimerHelp =
		"Starts when the start screen disappears. It measures elapsed wall-clock time, not the game's reported time.";
</script>

<section class={containerClass} aria-label="Wall-clock timers" aria-live="off" {role}>
	<div class={!wallClocks.sessionRunning ? inactiveTimerClass : ''} data-running={wallClocks.sessionRunning}>
		<small class={wallClocks.sessionRunning ? activeLabelClass : ''}>Time in session</small>
		<strong>{formatWallClockTime(wallClocks.sessionElapsedMs)}</strong>
	</div>
	<div class={!wallClocks.levelRunning ? inactiveTimerClass : ''} data-running={wallClocks.levelRunning}>
		<small
			class="{levelLabelClass} {wallClocks.levelRunning ? activeLabelClass : ''}"
			title={levelTimerHelp}
			aria-label={`Time in level. ${levelTimerHelp}`}>Time in level</small
		>
		<strong>{formatWallClockTime(wallClocks.levelElapsedMs)}</strong>
	</div>
</section>
