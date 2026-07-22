<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorTransitionStory from './MonitorTransitionStory.svelte';

	const { Story } = defineMeta({
		title: 'Monitor/State transitions',
		component: MonitorTransitionStory,
		parameters: { layout: 'fullscreen' },
		args: { outcome: 'complete', design: 'signal-band', stepDurationMs: 1600, loop: true }
	});
</script>

<Story name="Signal band · completed run" />
<Story name="Signal band · rapid states" args={{ stepDurationMs: 180 }} />
<Story name="Signal band · KIA run" args={{ outcome: 'kia' }} />
<Story name="Mission glass · completed run" args={{ design: 'mission-glass' }} />
<Story name="Mission glass · rapid states" args={{ design: 'mission-glass', stepDurationMs: 180 }} />
<Story name="Mission glass · aborted run" args={{ design: 'mission-glass', outcome: 'aborted' }} />
