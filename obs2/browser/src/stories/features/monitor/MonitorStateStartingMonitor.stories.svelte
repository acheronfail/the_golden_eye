<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorView from '$lib/features/monitor/MonitorView.svelte';
	import { monitorBaseArgs, monitorDesignArgs } from './monitorStoryFixtures';

	const { Story } = defineMeta({
		title: 'Monitor/Monitor states/Starting monitor',
		component: MonitorView,
		parameters: { layout: 'fullscreen' },
		args: { ...monitorBaseArgs, monitoring: false, transition: 'starting' }
	});
</script>

<Story name="Mission glass" args={monitorDesignArgs.missionGlass} />
<Story name="Signal band" args={monitorDesignArgs.signalBand} />
<Story name="For Your Eyes Only" args={monitorDesignArgs.debug} />
