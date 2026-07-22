<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import type { LevelMatch } from '$lib/api';
	import MonitorView from '$lib/components/MonitorView.svelte';

	const match = (screen: string, times: LevelMatch['times'] = null): LevelMatch => ({
		screen,
		mission: 2,
		part: 1,
		difficulty: 0,
		detected_lang: 'en',
		times,
		runtime_ms: 8.4
	});

	const { Story } = defineMeta({
		title: 'Monitor/Monitor states',
		component: MonitorView,
		parameters: { layout: 'fullscreen' },
		args: {
			verified: true,
			monitoring: true,
			recordingState: null,
			match: match('unknown'),
			onStop: () => {}
		}
	});
</script>

<Story name="Verifying source" args={{ verified: false, monitoring: false }} />
<Story name="Starting monitor" args={{ monitoring: false, transition: 'starting' }} />
<Story name="Waiting" />
<Story name="Recording" args={{ recordingState: 'started', match: match('start') }} />
<Story name="Cancelled" args={{ recordingState: 'cancelled', match: match('level select') }} />
<Story name="Failed" args={{ recordingState: 'failed', match: match('failed') }} />
<Story name="Aborted" args={{ recordingState: 'aborted', match: match('abort') }} />
<Story name="Killed in action" args={{ recordingState: 'kia', match: match('kia') }} />
<Story
	name="Complete"
	args={{
		recordingState: 'complete',
		match: match('stats', { time: 58, target_time: 65, best_time: 61 })
	}}
/>
<Story name="Skipped stats" args={{ recordingState: 'statsSkipped', match: match('level select') }} />
<Story name="Saving clip" args={{ recordingState: 'savePending', match: match('stats') }} />
<Story name="Stopping monitor" args={{ transition: 'stopping' }} />
<Story
	name="Healthy monitor FPS"
	args={{
		recordingState: 'started',
		match: match('start'),
		showMonitorFps: true,
		fps: { processedFps: 60, sourceFps: 60 }
	}}
/>
<Story
	name="Lagging monitor FPS"
	args={{
		recordingState: 'started',
		match: match('start'),
		showMonitorFps: true,
		fps: { processedFps: 43.2, sourceFps: 60 }
	}}
/>
