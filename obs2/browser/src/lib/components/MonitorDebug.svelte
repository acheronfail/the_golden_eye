<script lang="ts">
	import type { MonitorViewProps } from './monitorView';
	import { formatMonitorTime, monitorPresentation } from './monitorView';
	import RecentRuns from './RecentRuns.svelte';

	let {
		sourceName,
		verified,
		monitoring,
		transition = null,
		recordingState = null,
		match = null,
		fps = null,
		recentRuns = [],
		recentRunsBusyId = null,
		recentRunsError = null,
		onKeepRun = () => {},
		onStop
	}: MonitorViewProps = $props();

	const presentation = $derived(
		monitorPresentation({
			sourceName,
			verified,
			monitoring,
			transition,
			recordingState,
			match,
			fps,
			onStop
		})
	);
	const value = (input: unknown): string => (input == null ? 'null' : String(input));
	const seconds = (input: number | null | undefined): string =>
		input == null ? 'null' : `${input} s (${formatMonitorTime(input)})`;
	const matchJson = $derived(match ? JSON.stringify(match, null, 2) : 'null');
	const labelClass = 'text-[0.65rem] tracking-[0.1em] text-(--obs-text-dim) uppercase';
	const gridClass =
		'grid grid-cols-[repeat(auto-fit,minmax(9rem,1fr))] border-t border-l border-(--obs-border-muted) [&>div]:min-w-0 [&>div]:border-r [&>div]:border-b [&>div]:border-(--obs-border-muted) [&>div]:bg-(--obs-bg-elevated) [&>div]:px-2 [&>div]:py-1.5 [&>.state-cell]:bg-[color-mix(in_srgb,var(--debug-accent)_10%,var(--obs-bg-elevated))] [&_dt]:text-[0.65rem] [&_dt]:tracking-[0.1em] [&_dt]:text-(--obs-text-dim) [&_dt]:uppercase [&_dd]:mt-0.5 [&_dd]:text-(--obs-text) [&_dd]:[font-variant-numeric:tabular-nums] [&_dd]:[overflow-wrap:anywhere] [&_.state-cell_dd]:font-semibold [&_.state-cell_dd]:text-[color-mix(in_srgb,var(--debug-accent)_72%,var(--obs-text))]';

	const valueKind = (input: unknown): 'true' | 'false' | 'null' | 'value' => {
		if (input === true) return 'true';
		if (input === false) return 'false';
		if (input == null) return 'null';
		return 'value';
	};
</script>

{#snippet scalar(input: unknown, display = value(input))}
	<span data-value-kind={valueKind(input)}>{display}</span>
{/snippet}

<main
	class="monitor-debug @container relative h-full min-h-0 overflow-auto bg-(--obs-bg) p-3 font-mono text-xs text-(--obs-text) [--debug-accent:var(--obs-monitor-waiting)] data-[phase=complete]:[--debug-accent:var(--obs-gold-hover)] data-[phase=danger]:[--debug-accent:var(--obs-danger)] data-[phase=neutral]:[--debug-accent:var(--obs-text-muted)] data-[phase=recording]:[--debug-accent:var(--obs-success)] [&_[data-value-kind=false]]:text-(--obs-danger) [&_[data-value-kind=null]]:text-(--obs-text-dim) [&_[data-value-kind=null]]:italic [&_[data-value-kind=true]]:text-(--obs-success)"
	data-phase={presentation.phase}
	aria-live="polite"
>
	<header class="flex items-start justify-between gap-4 border-b-2 border-(--debug-accent) pb-3">
		<div>
			<p class={labelClass}>FOR YOUR EYES ONLY</p>
			<h1 class="mt-0.5 text-2xl font-semibold text-(--debug-accent)">
				{verified ? presentation.title : 'checking source'}
			</h1>
		</div>
		<button
			type="button"
			class="obs-button obs-button-danger min-h-10 px-4 py-2 text-xs"
			disabled={!monitoring || transition === 'stopping'}
			aria-label="Stop monitoring"
			onclick={onStop}
		>
			{transition === 'stopping' ? 'stopping' : 'stop'}
		</button>
	</header>

	<RecentRuns
		variant="debug"
		runs={recentRuns}
		busyRunId={recentRunsBusyId}
		error={recentRunsError}
		onKeep={onKeepRun}
	/>

	<section class="mt-3" aria-labelledby="lifecycle-heading">
		<h2 class="mb-1 {labelClass}" id="lifecycle-heading">Lifecycle</h2>
		<dl class={gridClass}>
			<div>
				<dt>source</dt>
				<dd>{@render scalar(sourceName)}</dd>
			</div>
			<div>
				<dt>verified</dt>
				<dd>{@render scalar(verified)}</dd>
			</div>
			<div>
				<dt>monitoring</dt>
				<dd>{@render scalar(monitoring)}</dd>
			</div>
			<div class="state-cell">
				<dt>transition</dt>
				<dd>{@render scalar(transition)}</dd>
			</div>
			<div class="state-cell">
				<dt>recording state</dt>
				<dd>{@render scalar(recordingState)}</dd>
			</div>
			<div class="state-cell">
				<dt>visual phase</dt>
				<dd>{presentation.phase}</dd>
			</div>
			<div class="state-cell">
				<dt>status label</dt>
				<dd>{presentation.statusLabel}</dd>
			</div>
		</dl>
	</section>

	<section class="mt-3" aria-labelledby="match-heading">
		<h2 class="mb-1 {labelClass}" id="match-heading">Current match</h2>
		<dl class={gridClass}>
			<div class="state-cell">
				<dt>screen</dt>
				<dd>{@render scalar(match?.screen)}</dd>
			</div>
			<div>
				<dt>mission</dt>
				<dd>{@render scalar(match?.mission)}</dd>
			</div>
			<div>
				<dt>part</dt>
				<dd>{@render scalar(match?.part)}</dd>
			</div>
			<div>
				<dt>difficulty</dt>
				<dd>{@render scalar(match?.difficulty)}</dd>
			</div>
			<div>
				<dt>language</dt>
				<dd>{@render scalar(match?.detected_lang)}</dd>
			</div>
			<div>
				<dt>runtime</dt>
				<dd>{@render scalar(match?.runtime_ms, match ? `${match.runtime_ms} ms` : 'null')}</dd>
			</div>
			<div>
				<dt>time</dt>
				<dd>{@render scalar(match?.times?.time, seconds(match?.times?.time))}</dd>
			</div>
			<div>
				<dt>target</dt>
				<dd>{@render scalar(match?.times?.target_time, seconds(match?.times?.target_time))}</dd>
			</div>
			<div>
				<dt>best</dt>
				<dd>{@render scalar(match?.times?.best_time, seconds(match?.times?.best_time))}</dd>
			</div>
			<div>
				<dt>raw times</dt>
				<dd>{@render scalar(match?.raw_times, match?.raw_times ? JSON.stringify(match.raw_times) : 'null')}</dd>
			</div>
			<div>
				<dt>match regions</dt>
				<dd>{@render scalar(match?.match_regions?.length)}</dd>
			</div>
			<div>
				<dt>annotation sets</dt>
				<dd>{@render scalar(match?.annotation_sets?.length)}</dd>
			</div>
		</dl>
	</section>

	<section class="mt-3" aria-labelledby="fps-heading">
		<h2 class="mb-1 {labelClass}" id="fps-heading">Frame processing</h2>
		<dl class={gridClass}>
			<div>
				<dt>processed FPS</dt>
				<dd>{@render scalar(fps?.processedFps)}</dd>
			</div>
			<div>
				<dt>source FPS</dt>
				<dd>{@render scalar(fps?.sourceFps)}</dd>
			</div>
			<div>
				<dt>lagging</dt>
				<dd>{@render scalar(presentation.fpsLagging)}</dd>
			</div>
		</dl>
	</section>

	<section class="mt-3" aria-labelledby="payload-heading">
		<h2 class="mb-1 {labelClass}" id="payload-heading">Raw match payload</h2>
		<pre
			class="m-0 border border-(--obs-border-muted) bg-(--obs-bg-elevated) p-2.5 font-[inherit] [overflow-wrap:anywhere] whitespace-pre-wrap text-(--obs-text-muted)">{matchJson}</pre>
	</section>
</main>
