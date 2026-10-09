<script lang="ts">
	import { onMount, tick } from 'svelte';
	import type { RecordingStatus } from '$lib/api';
	import MonitorView from '$lib/features/monitor/MonitorView.svelte';
	import { MonitorRunContext } from '$lib/features/monitor/monitorRunContext.svelte';
	import type { MonitorDesign } from '$lib/features/monitor/monitorView';
	import { monitorBaseArgs, monitorClockState, monitorMatch } from './monitorStoryFixtures';

	type Scenario =
		| 'restored'
		| 'waiting'
		| 'start'
		| 'startMissing'
		| 'playing'
		| 'playingMissing'
		| 'complete'
		| 'failed'
		| 'aborted'
		| 'kia'
		| 'stats'
		| 'cancelled'
		| 'statsSkipped'
		| 'selection';
	let { design, scenario }: { design: MonitorDesign; scenario: Scenario } = $props();
	const restoredContext = new MonitorRunContext();
	restoredContext.update(monitorMatch('start'), 'started');
	restoredContext.update(monitorMatch('unknown'), 'started');
	let launched = $state(false);
	const missing = $derived(scenario === 'startMissing' || scenario === 'playingMissing');
	const recordingState = $derived<RecordingStatus | null>(
		scenario === 'waiting'
			? null
			: !launched
				? 'started'
				: scenario === 'selection'
					? null
					: ['start', 'startMissing', 'playing', 'playingMissing', 'restored'].includes(scenario)
						? 'started'
						: scenario === 'stats'
							? 'complete'
							: (scenario as RecordingStatus)
	);
	const screen = $derived(
		scenario === 'waiting'
			? 'unknown'
			: !launched || scenario === 'start' || scenario === 'startMissing'
				? 'start'
				: scenario === 'selection'
					? 'select'
					: scenario === 'aborted'
						? 'abort'
						: ['waiting', 'playing', 'playingMissing', 'cancelled', 'statsSkipped'].includes(scenario)
							? 'unknown'
							: scenario
	);
	const playing = $derived(launched && (scenario === 'playing' || scenario === 'playingMissing'));
	const match = $derived(
		monitorMatch(screen, scenario === 'stats' && launched ? { time: 58, target_time: 65, best_time: 61 } : null)
	);

	onMount(() => {
		void tick().then(() => {
			launched = true;
		});
	});
</script>

<MonitorView
	{...monitorBaseArgs}
	{design}
	{recordingState}
	match={scenario === 'restored' ? monitorMatch('unknown') : match}
	runContext={scenario === 'restored' ? restoredContext : undefined}
	bestTimes={missing ? [] : monitorBaseArgs.bestTimes}
	showInGameTimer={true}
	wallClockState={{
		...monitorClockState,
		sessionElapsedMs: 1_345_000,
		levelElapsedMs: playing ? 23_456 : 0,
		levelTimerPhase: playing
			? 'running'
			: screen === 'start'
				? 'awaitingInitialBlack'
				: scenario === 'waiting' || scenario === 'selection'
					? 'idle'
					: 'stopped'
	}}
/>
