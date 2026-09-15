<script lang="ts">
	import type { LevelMatch, RecordingStatus } from '$lib/api';

	let {
		connected = false,
		enabled = false,
		recordingState = null,
		times = null,
		showTimes = true,
		transition = null,
		error = null,
		fontSize = 16
	}: {
		connected?: boolean;
		enabled?: boolean;
		recordingState?: RecordingStatus | null;
		times?: LevelMatch['times'];
		showTimes?: boolean;
		transition?: 'starting' | 'stopping' | null;
		error?: string | null;
		fontSize?: number;
	} = $props();

	const formatTime = (seconds: number | null | undefined): string =>
		seconds == null
			? '--:--'
			: `${Math.floor(seconds / 60)
					.toString()
					.padStart(2, '0')}:${(seconds % 60).toString().padStart(2, '0')}`;
	const visibleTimes = $derived(connected && enabled ? times : null);

	const banner = `\
┏┳┓┓     ┏┓  ┓ ┓      ┏┓
 ┃ ┣┓┏┓  ┃┓┏┓┃┏┫┏┓┏┓  ┣ ┓┏┏┓
 ┻ ┛┗┗   ┗┛┗┛┗┗┻┗ ┛┗  ┗┛┗┫┗
                         ┛`;
	const status = $derived.by(() => {
		if (!connected) return { label: 'Disconnected', dot: 'obs-phase-neutral-dot' };
		if (error) return { label: error, dot: 'obs-phase-danger-dot' };
		if (transition)
			return {
				label: transition === 'starting' ? 'Starting monitor…' : 'Stopping monitor…',
				dot: 'obs-phase-neutral-dot'
			};
		if (!enabled) return { label: 'Not monitoring', dot: 'obs-phase-neutral-dot' };
		switch (recordingState) {
			case 'started':
				return { label: 'Recording', dot: 'obs-phase-recording-dot' };
			case 'complete':
				return { label: 'Complete', dot: 'obs-phase-gold-dot' };
			case 'failed':
				return { label: 'Failed', dot: 'obs-phase-danger-dot' };
			case 'aborted':
				return { label: 'Aborted', dot: 'obs-phase-danger-dot' };
			case 'kia':
				return { label: 'Killed in action', dot: 'obs-phase-danger-dot' };
			case 'statsSkipped':
				return { label: 'Skipped stats', dot: 'obs-phase-danger-dot' };
			case 'cancelled':
				return { label: 'Cancelled', dot: 'obs-phase-neutral-dot' };
			default:
				return { label: 'Waiting', dot: 'obs-phase-waiting-dot' };
		}
	});
</script>

<main
	data-minimal-overlay
	class="flex h-dvh w-full items-center justify-center overflow-hidden bg-transparent font-mono text-(length:--overlay-font-size)"
	style:--overlay-font-size="{fontSize}px"
>
	<div class="flex flex-col items-center gap-[0.75em]">
		<pre
			aria-label="the golden eye"
			class="text-left text-[0.625em] leading-[1.17] whitespace-pre text-(--obs-gold)">{banner}</pre>
		<div
			role="status"
			class="flex items-center gap-[0.5em] font-mono text-[2.5em] leading-tight font-medium whitespace-nowrap text-(--obs-text)"
		>
			<span aria-hidden="true" class="size-[0.5em] shrink-0 rounded-full {status.dot}"></span>
			<span>{status.label}</span>
		</div>
		{#if showTimes}
			<div
				aria-label="Recognised times"
				class="flex items-center gap-[1em] font-mono text-[1.25em] leading-tight whitespace-nowrap text-(--obs-text)"
			>
				<span aria-label="Run time" title="Run time">{formatTime(visibleTimes?.time)}</span>
				<span aria-label="Target time" title="Target time">{formatTime(visibleTimes?.target_time)}</span>
				<span aria-label="Best time" title="Best time">{formatTime(visibleTimes?.best_time)}</span>
			</div>
		{/if}
	</div>
</main>
