<script lang="ts">
	import type { MonitorViewProps } from './monitorView';
	import { formatMonitorTime, monitorPresentation } from './monitorView';

	let {
		sourceName,
		verified,
		monitoring,
		transition = null,
		recordingState = null,
		match = null,
		fps = null,
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

<main class="monitor-debug" data-phase={presentation.phase} aria-live="polite">
	<header>
		<div>
			<p>FOR YOUR EYES ONLY</p>
			<h1>{verified ? presentation.title : 'checking source'}</h1>
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

	<section aria-labelledby="lifecycle-heading">
		<h2 id="lifecycle-heading">Lifecycle</h2>
		<dl>
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

	<section aria-labelledby="match-heading">
		<h2 id="match-heading">Current match</h2>
		<dl>
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

	<section aria-labelledby="fps-heading">
		<h2 id="fps-heading">Frame processing</h2>
		<dl>
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

	<section aria-labelledby="payload-heading">
		<h2 id="payload-heading">Raw match payload</h2>
		<pre>{matchJson}</pre>
	</section>
</main>

<style>
	.monitor-debug {
		--debug-accent: var(--obs-monitor-waiting);
		height: 100%;
		min-height: 0;
		overflow: auto;
		padding: 0.75rem;
		background: var(--obs-bg);
		color: var(--obs-text);
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.75rem;
	}

	.monitor-debug[data-phase='recording'] {
		--debug-accent: var(--obs-success);
	}
	.monitor-debug[data-phase='complete'] {
		--debug-accent: var(--obs-gold-hover);
	}
	.monitor-debug[data-phase='danger'] {
		--debug-accent: var(--obs-danger);
	}
	.monitor-debug[data-phase='neutral'] {
		--debug-accent: var(--obs-text-muted);
	}

	header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 1rem;
		padding-bottom: 0.75rem;
		border-bottom: 2px solid var(--debug-accent);
	}

	header p,
	h2,
	dt {
		color: var(--obs-text-dim);
		font-size: 0.65rem;
		letter-spacing: 0.1em;
		text-transform: uppercase;
	}

	h1 {
		margin-top: 0.2rem;
		color: var(--debug-accent);
		font-size: 1.5rem;
		font-weight: 600;
	}

	section {
		margin-top: 0.75rem;
	}
	h2 {
		margin-bottom: 0.35rem;
	}

	dl {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
		border-top: 1px solid var(--obs-border-muted);
		border-left: 1px solid var(--obs-border-muted);
	}

	dl div {
		min-width: 0;
		padding: 0.4rem 0.5rem;
		border-right: 1px solid var(--obs-border-muted);
		border-bottom: 1px solid var(--obs-border-muted);
		background: var(--obs-bg-elevated);
	}

	dl .state-cell {
		background: color-mix(in srgb, var(--debug-accent) 10%, var(--obs-bg-elevated));
	}

	.state-cell dd {
		color: color-mix(in srgb, var(--debug-accent) 72%, var(--obs-text));
		font-weight: 600;
	}

	[data-value-kind='true'] {
		color: var(--obs-success);
	}

	[data-value-kind='false'] {
		color: var(--obs-danger);
	}

	[data-value-kind='null'] {
		color: var(--obs-text-dim);
		font-style: italic;
	}

	dd {
		margin-top: 0.2rem;
		color: var(--obs-text);
		font-variant-numeric: tabular-nums;
		overflow-wrap: anywhere;
	}

	pre {
		margin: 0;
		padding: 0.6rem;
		overflow-wrap: anywhere;
		border: 1px solid var(--obs-border-muted);
		background: var(--obs-bg-elevated);
		color: var(--obs-text-muted);
		font: inherit;
		white-space: pre-wrap;
	}
</style>
