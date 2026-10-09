<script lang="ts" module>
	export type { MonitorDesign, MonitorTransition, MonitorViewProps } from './monitorView';
</script>

<script lang="ts">
	import { clipTimeSeconds } from '$lib/features/runs/runsView';
	import { onDestroy, untrack } from 'svelte';
	import { DEFAULT_SETTINGS } from '$lib/generated/settings';
	import MonitorDebug from './MonitorDebug.svelte';
	import MonitorMissionGlass from './MonitorMissionGlass.svelte';
	import MonitorSignalBand from './MonitorSignalBand.svelte';
	import { monitorRunIdentityLabel } from './monitorRunIdentity';
	import { MonitorRunContext } from './monitorRunContext.svelte';
	import { MonitorWallClocks } from './monitorWallClocks.svelte';
	import type { MonitorDesign, MonitorViewProps } from './monitorView';

	let {
		design = DEFAULT_SETTINGS.monitorDesign,
		showMonitorFps = DEFAULT_SETTINGS.showMonitorFps,
		showInGameTimer = DEFAULT_SETTINGS.showInGameTimer,
		...props
	}: MonitorViewProps & { design?: MonitorDesign } = $props();
	const wallClocks = new MonitorWallClocks();
	const localRunContext = new MonitorRunContext();
	const runContext = $derived(props.runContext ?? localRunContext);
	const runIdentity = $derived(runContext.identity);
	const showPersonalBest = $derived(
		props.monitoring && props.verified && !props.transition && runContext.personalBestIdentity !== null
	);
	const personalBest = $derived(
		runIdentity
			? (props.bestTimes ?? [])
					.filter(
						(run) =>
							run.metadata.level === runIdentity?.level &&
							run.metadata.difficulty === runIdentity?.difficulty &&
							run.metadata.status === 'complete'
					)
					.map(clipTimeSeconds)
					.filter((time): time is number => time !== null)
					.reduce<number | null>((best, time) => (best === null ? time : Math.min(best, time)), null)
			: null
	);
	const runIdentityLabel = $derived(monitorRunIdentityLabel(runIdentity));

	$effect(() => {
		if (props.wallClockState) {
			wallClocks.sync(props.wallClockState);
		}
	});

	$effect(() => {
		if (props.runContext) return;
		const match = props.match;
		const recordingState = props.monitoring && props.verified && !props.transition ? props.recordingState : null;
		untrack(() => localRunContext.update(match, recordingState));
	});

	onDestroy(() => wallClocks.destroy());
</script>

{#if design === 'debug'}
	<MonitorDebug {...props} {personalBest} {showMonitorFps} {showInGameTimer} {wallClocks} />
{:else if design === 'mission-glass'}
	<MonitorMissionGlass
		{...props}
		{showMonitorFps}
		{showInGameTimer}
		{wallClocks}
		{personalBest}
		{showPersonalBest}
		{runIdentityLabel}
		runIdentityAvailable={runIdentity !== null}
	/>
{:else if design === 'signal-band'}
	<MonitorSignalBand
		{...props}
		{showMonitorFps}
		{showInGameTimer}
		{wallClocks}
		{personalBest}
		{showPersonalBest}
		{runIdentityLabel}
		runIdentityAvailable={runIdentity !== null}
	/>
{/if}
