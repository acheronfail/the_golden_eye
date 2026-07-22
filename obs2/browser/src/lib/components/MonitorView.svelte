<script lang="ts" module>
	export type { MonitorDesign, MonitorTransition, MonitorViewProps } from './monitorView';
</script>

<script lang="ts">
	import MonitorDebug from './MonitorDebug.svelte';
	import MonitorMissionGlass from './MonitorMissionGlass.svelte';
	import MonitorSignalBand from './MonitorSignalBand.svelte';
	import type { MonitorDesign, MonitorViewProps } from './monitorView';

	let { design = 'signal-band', ...props }: MonitorViewProps & { design?: MonitorDesign } = $props();
</script>

{#if design === 'debug'}
	<MonitorDebug {...props} />
{:else if design === 'mission-glass'}
	<MonitorMissionGlass {...props} />
{:else}
	<MonitorSignalBand {...props} />
{/if}
