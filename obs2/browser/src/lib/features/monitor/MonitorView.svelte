<script lang="ts" module>
	export type { MonitorDesign, MonitorTransition, MonitorViewProps } from './monitorView';
</script>

<script lang="ts">
	import { onDestroy } from 'svelte';
	import MonitorDebug from './MonitorDebug.svelte';
	import MonitorMissionGlass from './MonitorMissionGlass.svelte';
	import MonitorSignalBand from './MonitorSignalBand.svelte';
	import { MonitorWallClocks } from './monitorWallClocks.svelte';
	import type { MonitorDesign, MonitorViewProps } from './monitorView';

	let { design = 'signal-band', ...props }: MonitorViewProps & { design?: MonitorDesign } = $props();
	const wallClocks = new MonitorWallClocks();

	$effect(() => {
		wallClocks.reconcile(props.monitoring, props.match?.screen ?? null);
	});

	onDestroy(() => wallClocks.destroy());
</script>

{#if design === 'debug'}
	<MonitorDebug {...props} {wallClocks} />
{:else if design === 'mission-glass'}
	<MonitorMissionGlass {...props} {wallClocks} />
{:else if design === 'signal-band'}
	<MonitorSignalBand {...props} {wallClocks} />
{/if}
