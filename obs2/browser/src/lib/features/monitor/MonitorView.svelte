<script lang="ts" module>
	export type { MonitorDesign, MonitorTransition, MonitorViewProps } from './monitorView';
</script>

<script lang="ts">
	import { onDestroy, untrack } from 'svelte';
	import { DEFAULT_SETTINGS } from '$lib/generated/settings';
	import MonitorDebug from './MonitorDebug.svelte';
	import MonitorMissionGlass from './MonitorMissionGlass.svelte';
	import MonitorSignalBand from './MonitorSignalBand.svelte';
	import { monitorRunIdentityLabel, reconcileMonitorRunIdentity, type MonitorRunIdentity } from './monitorRunIdentity';
	import { MonitorWallClocks } from './monitorWallClocks.svelte';
	import type { MonitorDesign, MonitorViewProps } from './monitorView';

	let {
		design = DEFAULT_SETTINGS.monitorDesign,
		showMonitorFps = DEFAULT_SETTINGS.showMonitorFps,
		showInGameTimer = DEFAULT_SETTINGS.showInGameTimer,
		...props
	}: MonitorViewProps & { design?: MonitorDesign } = $props();
	const wallClocks = new MonitorWallClocks();
	let runIdentity = $state<MonitorRunIdentity | null>(null);
	const runIdentityLabel = $derived(monitorRunIdentityLabel(runIdentity));

	$effect(() => {
		if (props.wallClockState) {
			wallClocks.sync(props.wallClockState);
		}
	});

	$effect(() => {
		runIdentity = reconcileMonitorRunIdentity(
			untrack(() => runIdentity),
			props.match
		);
	});

	onDestroy(() => wallClocks.destroy());
</script>

{#if design === 'debug'}
	<MonitorDebug {...props} {showMonitorFps} {showInGameTimer} {wallClocks} />
{:else if design === 'mission-glass'}
	<MonitorMissionGlass
		{...props}
		{showMonitorFps}
		{showInGameTimer}
		{wallClocks}
		{runIdentityLabel}
		runIdentityAvailable={runIdentity !== null}
	/>
{:else if design === 'signal-band'}
	<MonitorSignalBand
		{...props}
		{showMonitorFps}
		{showInGameTimer}
		{wallClocks}
		{runIdentityLabel}
		runIdentityAvailable={runIdentity !== null}
	/>
{/if}
