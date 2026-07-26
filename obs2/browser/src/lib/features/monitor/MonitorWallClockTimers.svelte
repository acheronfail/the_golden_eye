<script lang="ts">
	import { formatWallClockTime, type MonitorWallClockSnapshot } from './monitorWallClocks.svelte';

	let {
		variant,
		wallClocks
	}: {
		variant: 'mission-glass' | 'signal-band';
		wallClocks: MonitorWallClockSnapshot;
	} = $props();

	const levelTimerHelp =
		"Starts when the start screen disappears. It measures elapsed wall-clock time, not the game's reported time.";
</script>

<section
	class="wall-clock-timers grid grid-cols-2 font-mono [&_small]:uppercase [&_strong]:font-medium [&_strong]:[font-variant-numeric:tabular-nums] [&>div]:grid [&>div]:gap-0.5 {variant ===
	'mission-glass'
		? 'wall-clock-timers--mission-glass gap-px overflow-hidden rounded-xl border border-[color-mix(in_srgb,var(--monitor-accent)_24%,var(--obs-border-soft))] bg-[color-mix(in_srgb,var(--monitor-accent)_18%,var(--obs-border-muted))] shadow-[0_0.75rem_2.5rem_rgb(0_0_0_/_18%)] [&_small]:text-[clamp(0.52rem,2cqw,0.65rem)] [&_small]:tracking-[0.12em] [&_strong]:text-[clamp(1rem,4.6cqw,1.55rem)] [&_strong]:tracking-[-0.035em] [&>div]:bg-[rgb(37_41_52_/_78%)] [&>div]:px-[clamp(0.75rem,3cqw,1.35rem)] [&>div]:py-[clamp(0.55rem,1.5cqh,0.8rem)] [@container(max-height:42rem)]:[&_strong]:text-[clamp(0.75rem,3cqw,0.95rem)] [@container(max-height:42rem)]:[&>div]:py-1 [@container(max-height:58rem)]:[&_strong]:text-[clamp(0.82rem,2.8cqw,1.1rem)] [@container(max-height:58rem)]:[&>div]:py-1.5'
		: 'wall-clock-timers--signal-band mb-[clamp(1rem,3cqh,2rem)] max-w-[34rem] gap-[clamp(1rem,5cqw,3rem)] [&_small]:text-[clamp(0.54rem,2cqw,0.68rem)] [&_small]:tracking-[0.14em] [&_strong]:text-[clamp(1.15rem,5cqw,2rem)] [&_strong]:tracking-[-0.04em] [@container(max-height:42rem)]:mb-2 [@container(max-height:42rem)]:[&_strong]:text-[clamp(1rem,4cqw,1.45rem)]'}"
	aria-label="Wall-clock timers"
	aria-live="off"
	role={variant === 'signal-band' ? 'group' : undefined}
>
	<div class:text-(--obs-text-dim)={!wallClocks.sessionRunning} data-running={wallClocks.sessionRunning}>
		<small class:text-(--monitor-accent)={wallClocks.sessionRunning}>Time in session</small>
		<strong>{formatWallClockTime(wallClocks.sessionElapsedMs)}</strong>
	</div>
	<div class:text-(--obs-text-dim)={!wallClocks.levelRunning} data-running={wallClocks.levelRunning}>
		<small
			class="w-fit cursor-help"
			class:text-(--monitor-accent)={wallClocks.levelRunning}
			title={levelTimerHelp}
			aria-label={`Time in level. ${levelTimerHelp}`}>Time in level</small
		>
		<strong>{formatWallClockTime(wallClocks.levelElapsedMs)}</strong>
	</div>
</section>
