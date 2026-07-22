<script lang="ts">
	import type { LevelMatch, RecordingStatus } from '$lib/api';
	import MonitorView from '$lib/components/MonitorView.svelte';

	type Outcome = 'complete' | 'aborted' | 'kia';
	type TransitionStep = {
		label: string;
		recordingState: RecordingStatus | null;
		match: LevelMatch;
	};

	let {
		outcome,
		stepDurationMs = 1600,
		loop = true
	}: { outcome: Outcome; stepDurationMs?: number; loop?: boolean } = $props();

	const levelMatch = (screen: string, times: LevelMatch['times'] = null): LevelMatch => ({
		screen,
		mission: 2,
		part: 1,
		difficulty: 0,
		detected_lang: 'en',
		times,
		runtime_ms: 8.4
	});

	const outcomeScreen = $derived(outcome === 'aborted' ? 'abort' : outcome);
	const steps = $derived<TransitionStep[]>([
		{ label: 'waiting', recordingState: null, match: levelMatch('unknown') },
		{ label: 'recording', recordingState: 'started', match: levelMatch('start') },
		{ label: outcome, recordingState: outcome, match: levelMatch(outcomeScreen) },
		{
			label: 'stats',
			recordingState: null,
			match: levelMatch('stats', { time: outcome === 'complete' ? 58 : 37, target_time: 65, best_time: 61 })
		}
	]);

	let stepIndex = $state(0);
	let paused = $state(false);
	const step = $derived(steps[stepIndex] ?? steps[0]);

	$effect(() => {
		outcome;
		stepIndex = 0;
	});

	$effect(() => {
		if (paused) return;
		const interval = window.setInterval(() => {
			if (stepIndex < steps.length - 1) {
				stepIndex += 1;
			} else if (loop) {
				stepIndex = 0;
			}
		}, stepDurationMs);
		return () => window.clearInterval(interval);
	});

	const replay = () => {
		stepIndex = 0;
		paused = false;
	};
</script>

<div class="flex h-full min-h-0 flex-col">
	<div class="min-h-0 flex-1">
		<MonitorView
			verified={true}
			monitoring={true}
			recordingState={step.recordingState}
			match={step.match}
			onStop={() => {}}
		/>
	</div>

	<footer class="obs-panel flex shrink-0 flex-wrap items-center justify-center gap-2 px-3 py-3 font-mono text-xs">
		<span class="obs-panel rounded px-2 py-1">{stepIndex + 1}/{steps.length} · {step.label}</span>
		<button type="button" class="obs-button obs-button-xs" onclick={() => (paused = !paused)}>
			{paused ? 'resume' : 'pause'}
		</button>
		<button type="button" class="obs-button obs-button-xs" onclick={replay}>replay</button>
	</footer>
</div>
