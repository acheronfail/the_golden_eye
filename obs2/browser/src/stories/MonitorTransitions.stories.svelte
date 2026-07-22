<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorTransitionStory from './MonitorTransitionStory.svelte';

	const { Story } = defineMeta({
		title: 'Monitor/State transitions',
		component: MonitorTransitionStory,
		parameters: { layout: 'fullscreen' },
		args: { outcome: 'complete', stepDurationMs: 1600, loop: true }
	});
</script>

<Story name="Completed run" />
<Story name="Aborted run" args={{ outcome: 'aborted' }} />
<Story name="KIA run" args={{ outcome: 'kia' }} />
