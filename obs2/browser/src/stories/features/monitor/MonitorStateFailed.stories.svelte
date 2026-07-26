<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorView from '$lib/features/monitor/MonitorView.svelte';
	import { monitorBaseArgs, monitorDesignArgs, monitorMatch as match } from './monitorStoryFixtures';

	const { Story } = defineMeta({
		title: 'Monitor/Monitor states/Failed',
		component: MonitorView,
		parameters: { layout: 'fullscreen' },
		args: { ...monitorBaseArgs, recordingState: 'failed', match: match('failed') }
	});
</script>

<Story name="Mission glass" args={monitorDesignArgs.missionGlass} />
<Story name="Signal band" args={monitorDesignArgs.signalBand} />
<Story name="For Your Eyes Only" args={monitorDesignArgs.debug} />
