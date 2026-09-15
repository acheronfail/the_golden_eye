<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MinimalOverlay from '$lib/features/minimal/MinimalOverlay.svelte';
	const { Story } = defineMeta({
		title: 'Minimal/Overlay',
		component: MinimalOverlay,
		parameters: { layout: 'fullscreen' },
		args: { connected: true, enabled: true }
	});
</script>

<Story name="Waiting" />
<Story name="Disconnected" args={{ connected: false }} />
<Story name="Not monitoring" args={{ enabled: false }} />
<Story name="Recording" args={{ recordingState: 'started' }} />
<Story name="Complete" args={{ recordingState: 'complete' }} />
<Story name="Failed" args={{ recordingState: 'failed' }} />
<Story name="Aborted" args={{ recordingState: 'aborted' }} />
<Story name="Killed in action" args={{ recordingState: 'kia' }} />
<Story name="Skipped stats" args={{ recordingState: 'statsSkipped' }} />
<Story name="Cancelled" args={{ recordingState: 'cancelled' }} />
<Story name="Large" args={{ fontSize: 32 }} />

<Story name="All times" args={{ recordingState: 'complete', times: { time: 83, target_time: 120, best_time: 75 } }} />
<Story
	name="Run time only"
	args={{ recordingState: 'complete', times: { time: 83, target_time: null, best_time: null } }}
/>
<Story name="Zero time" args={{ times: { time: 0, target_time: null, best_time: null } }} />

<Story name="Starting" args={{ transition: 'starting' }} />
<Story name="Stopping" args={{ transition: 'stopping' }} />
<Story name="Start error" args={{ enabled: false, error: 'Replay buffer is unavailable' }} />

<Story name="Times hidden" args={{ showTimes: false }} />
