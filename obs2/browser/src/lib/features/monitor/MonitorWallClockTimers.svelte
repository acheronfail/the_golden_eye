<script lang="ts">
	import Tooltip from '$lib/ui/Tooltip.svelte';
	import MetaPills from '../../ui/MetaPills.svelte';
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
		"GoldenEye's in-game timer can be inconsistent, so this time is only an estimate. It waits through the opening cutscenes, starts when gameplay should begin, and stops at the next fade to black.";
</script>

<section class={containerClass} aria-label="Wall-clock timers" aria-live="off" {role}>
	<div class={!wallClocks.sessionRunning ? inactiveTimerClass : ''} data-running={wallClocks.sessionRunning}>
		<small class={wallClocks.sessionRunning ? activeLabelClass : ''}>Time in session</small>
		<strong>{formatWallClockTime(wallClocks.sessionElapsedMs)}</strong>
	</div>
	<div class={!wallClocks.levelRunning ? inactiveTimerClass : ''} data-running={wallClocks.levelRunning}>
		<Tooltip content={levelTimerHelp} class={levelLabelClass}>
			<small class={wallClocks.levelRunning ? activeLabelClass : ''}>
				<span class="flex flex-row items-center gap-2">
					Time in level
					<MetaPills chips={[{ label: 'beta', class: 'text-(--obs-danger) border border-(--obs-danger)' }]} />
				</span>
			</small>
		</Tooltip>
		<strong>{formatWallClockTime(wallClocks.levelElapsedMs)}</strong>
	</div>
</section>
