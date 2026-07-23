<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import MonitorView from '$lib/components/MonitorView.svelte';
	import { longMonitorRecentRuns, monitorMatch as match } from './monitorStoryFixtures';
	import { completedRun, failedRun } from './fixtures';

	const { Story } = defineMeta({
		title: 'Monitor/Monitor states/Signal band',
		component: MonitorView,
		parameters: { layout: 'fullscreen' },
		args: {
			design: 'signal-band',
			sourceName: 'N64 Capture',
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
	args={{ recordingState: 'complete', match: match('stats', { time: 58, target_time: 65, best_time: 61 }) }}
/>
<Story name="Skipped stats" args={{ recordingState: 'statsSkipped', match: match('level select') }} />
<Story name="Saving clip" args={{ recordingState: 'savePending', match: match('stats') }} />
<Story
	name="Recent run history"
	args={{
		recentRuns: [
			{ ...failedRun, runId: 'recent-pending', retentionState: 'pending' },
			{ ...completedRun, runId: 'recent-ready', retentionState: 'pending' },
			{ ...completedRun, runId: 'recent-kept', retentionState: 'kept', retentionReason: 'manual' },
			{ ...completedRun, runId: 'recent-expired', path: '', retentionState: 'expired' },
			{ ...completedRun, runId: 'recent-pb', retentionState: 'kept', retentionReason: 'personalBest' }
		]
	}}
/>
<Story
	name="Stats with long recent history"
	args={{
		recordingState: 'complete',
		match: match('stats', { time: 58, target_time: 65, best_time: 61 }),
		recentRuns: longMonitorRecentRuns
	}}
/>
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
