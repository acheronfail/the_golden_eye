<script lang="ts">
	import Tooltip from '$lib/ui/Tooltip.svelte';
	import MetaPills from '$lib/ui/MetaPills.svelte';
	import { formatWallClockTime, type MonitorWallClockSnapshot } from './monitorWallClocks.svelte';

	let {
		wallClocks,
		containerClass = '',
		inactiveTimerClass = '',
		activeLabelClass = '',
		showInGameTimer,
		role
	}: {
		wallClocks: MonitorWallClockSnapshot;
		containerClass?: string;
		inactiveTimerClass?: string;
		activeLabelClass?: string;
		showInGameTimer: boolean;
		role?: 'group';
	} = $props();

	const levelTimerHelp =
		"GoldenEye's in-game timer can be inconsistent, so this time is only an estimate. It waits through the opening cutscenes, pauses while the watch is open, and stops at the next fade to black.";
</script>

<section
	class={`${containerClass} ${!showInGameTimer ? '[&>div]:col-span-full' : ''}`}
	aria-label="Wall-clock timers"
	aria-live="off"
	{role}
>
	<div class={!wallClocks.sessionRunning ? inactiveTimerClass : ''} data-running={wallClocks.sessionRunning}>
		<small class={wallClocks.sessionRunning ? activeLabelClass : ''}>Time in session</small>
		<strong>{formatWallClockTime(wallClocks.sessionElapsedMs)}</strong>
	</div>
	{#if showInGameTimer}
		<div class="relative {!wallClocks.levelRunning ? inactiveTimerClass : ''}" data-running={wallClocks.levelRunning}>
			<Tooltip
				content={levelTimerHelp}
				class="h-full w-full cursor-help flex-col items-start gap-0.5 before:absolute before:inset-0"
			>
				<small class={wallClocks.levelRunning ? activeLabelClass : ''}>
					<span class="flex flex-row items-center gap-2">
						Time in level
						<MetaPills chips={[{ label: 'beta', class: 'text-(--obs-danger) border border-(--obs-danger)' }]} />
					</span>
				</small>
				<strong>{formatWallClockTime(wallClocks.levelElapsedMs)}</strong>
			</Tooltip>
		</div>
	{/if}
</section>
