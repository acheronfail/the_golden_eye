<script lang="ts">
	import type { MonitorViewProps } from './monitorView';
	import { formatMonitorTime, monitorPresentation } from './monitorView';

	let {
		verified,
		monitoring,
		transition = null,
		recordingState = null,
		match = null,
		fps = null,
		showMonitorFps = false,
		onStop
	}: MonitorViewProps = $props();

	const presentation = $derived(
		monitorPresentation({ verified, monitoring, transition, recordingState, match, fps, showMonitorFps, onStop })
	);
</script>

<main
	class="monitor-signal-band"
	data-phase={presentation.phase}
	aria-busy={presentation.waitingForObs || !verified}
	aria-live="polite"
>
	<div class="signal-atmosphere" aria-hidden="true"></div>
	<div class="signal-rail" aria-hidden="true"><span></span></div>

	<header class="monitor-topbar">
		<span class="monitor-brand">
			<span class="brand-light" aria-hidden="true"></span>
			<span class="brand-full">GOLDENEYE MONITOR</span>
			<span class="brand-short">GE MONITOR</span>
		</span>
		<button
			type="button"
			class="obs-button obs-button-danger min-h-10 px-4 py-2 text-xs shadow-lg shadow-black/25"
			disabled={!monitoring || transition === 'stopping'}
			aria-label="Stop monitoring"
			onclick={onStop}
		>
			<span class="stop-square" aria-hidden="true"></span>
			{transition === 'stopping' ? 'stopping' : 'stop'}
		</button>
	</header>

	{#key presentation.animationKey}
		<div class="signal-sweep" aria-hidden="true"></div>
		<section class="signal-content">
			<p class="signal-kicker">{verified ? presentation.statusLabel : 'Verifying source'} / ACTIVE</p>
			<h1>{verified ? presentation.title : 'checking source'}</h1>
			{#if presentation.showDetail && verified}
				<p class="signal-detail">{presentation.detail}</p>
			{/if}
		</section>
	{/key}

	{#if match?.times && !presentation.waitingForObs}
		<div class="signal-metrics" aria-label="Run times">
			<span>
				<small>time</small>
				<strong>{formatMonitorTime(match.times.time)}</strong>
			</span>
			{#if match.times.target_time != null}
				<span>
					<small>target</small>
					<strong>{formatMonitorTime(match.times.target_time)}</strong>
				</span>
			{/if}
			{#if match.times.best_time != null}
				<span>
					<small>best</small>
					<strong>{formatMonitorTime(match.times.best_time)}</strong>
				</span>
			{/if}
		</div>
	{/if}

	<footer class="monitor-footer">
		<span>{presentation.phase}</span>
		{#if monitoring}
			<span class="stop-hint">escape or space to stop</span>
		{/if}
		{#if monitoring && showMonitorFps && presentation.fpsText}
			<span class:fps-lagging={presentation.fpsLagging}>{presentation.fpsText}</span>
		{/if}
	</footer>
</main>

<style>
	.monitor-signal-band {
		--monitor-accent: var(--obs-monitor-waiting);
		--monitor-surface: var(--obs-monitor-waiting-surface);
		position: relative;
		isolation: isolate;
		display: flex;
		height: 100%;
		min-height: 0;
		container-type: inline-size;
		overflow: hidden;
		background: var(--obs-bg);
		color: var(--obs-text);
		transition: background-color 240ms ease;
	}

	.monitor-signal-band[data-phase='recording'] {
		--monitor-accent: var(--obs-success);
		--monitor-surface: var(--obs-success-surface);
	}

	.monitor-signal-band[data-phase='complete'] {
		--monitor-accent: var(--obs-gold-hover);
		--monitor-surface: var(--obs-gold-surface);
	}

	.monitor-signal-band[data-phase='danger'] {
		--monitor-accent: var(--obs-danger);
		--monitor-surface: var(--obs-danger-surface);
	}

	.monitor-signal-band[data-phase='neutral'] {
		--monitor-accent: var(--obs-text-muted);
		--monitor-surface: rgb(182 186 196 / 11%);
	}

	.signal-atmosphere {
		position: absolute;
		inset: 0;
		z-index: -3;
		background:
			linear-gradient(115deg, var(--monitor-surface), transparent 44%),
			radial-gradient(circle at 22% 54%, var(--monitor-surface), transparent 45%);
		transition: background 240ms ease;
	}

	.signal-atmosphere::after {
		position: absolute;
		inset: 0;
		background-image: linear-gradient(rgb(255 255 255 / 2%) 1px, transparent 1px);
		background-size: 100% 4px;
		content: '';
		mask-image: linear-gradient(to bottom, transparent, #000 30%, #000 70%, transparent);
	}

	.signal-rail {
		position: absolute;
		top: 1rem;
		bottom: 1rem;
		left: 1rem;
		z-index: -1;
		width: 0.3rem;
		overflow: hidden;
		border-radius: 999px;
		background: color-mix(in srgb, var(--monitor-accent) 22%, var(--obs-border));
		box-shadow: 0 0 2rem var(--monitor-surface);
		transition:
			background-color 240ms ease,
			box-shadow 240ms ease;
	}

	.signal-rail span {
		display: block;
		width: 100%;
		height: 42%;
		background: var(--monitor-accent);
		box-shadow: 0 0 1.1rem var(--monitor-accent);
		transition:
			background-color 240ms ease,
			box-shadow 240ms ease;
	}

	.monitor-topbar,
	.monitor-footer {
		position: absolute;
		right: 1.5rem;
		left: 2rem;
		z-index: 4;
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 1rem;
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: 0.7rem;
		letter-spacing: 0.12em;
		text-transform: uppercase;
	}

	.monitor-topbar {
		top: 1.5rem;
	}
	.monitor-footer {
		bottom: 1.5rem;
		color: var(--obs-text-dim);
	}

	.monitor-brand {
		white-space: nowrap;
	}

	.brand-short {
		display: none;
	}

	.brand-light {
		display: inline-block;
		width: 0.55rem;
		height: 0.55rem;
		margin-right: 0.3rem;
		border-radius: 50%;
		background: var(--monitor-accent);
		box-shadow: 0 0 0.9rem var(--monitor-accent);
		vertical-align: -0.02rem;
		transition:
			background-color 240ms ease,
			box-shadow 240ms ease;
	}

	.stop-square {
		width: 0.5rem;
		height: 0.5rem;
		margin-right: 0.45rem;
		border-radius: 1px;
		background: currentColor;
	}

	.signal-content {
		position: absolute;
		top: 50%;
		right: clamp(1.75rem, 7cqw, 5rem);
		left: clamp(2.5rem, 9cqw, 7rem);
		transform: translateY(-50%);
		animation: signal-title-in 460ms cubic-bezier(0.2, 0.82, 0.2, 1) both;
	}

	.signal-kicker,
	.signal-detail {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: clamp(0.65rem, 2.8cqw, 0.82rem);
		letter-spacing: 0.15em;
		text-transform: uppercase;
	}

	.signal-kicker {
		color: var(--monitor-accent);
		transition: color 240ms ease;
	}

	h1 {
		max-width: 100%;
		margin: 0.5rem 0 0.65rem;
		color: color-mix(in srgb, var(--monitor-accent) 12%, var(--obs-text));
		font-size: clamp(3rem, 17cqw, 7.5rem);
		font-weight: 600;
		line-height: 0.9;
		letter-spacing: -0.065em;
		overflow-wrap: anywhere;
		transition: color 240ms ease;
	}

	.signal-detail {
		color: var(--obs-text-dim);
	}

	.signal-sweep {
		position: absolute;
		inset: 0;
		z-index: -2;
		background: linear-gradient(90deg, transparent, var(--monitor-surface), transparent);
		animation: signal-wipe 520ms ease-out both;
		pointer-events: none;
	}

	.signal-metrics {
		position: absolute;
		right: 1.75rem;
		bottom: 5rem;
		left: clamp(2.5rem, 9cqw, 7rem);
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: clamp(0.75rem, 4cqw, 2.5rem);
		animation: metrics-in 400ms 80ms ease-out both;
		font-family: var(--font-mono, ui-monospace, monospace);
	}

	.signal-metrics span {
		display: grid;
		gap: 0.25rem;
		min-width: 0;
	}
	.signal-metrics small {
		color: var(--obs-text-dim);
		font-size: 0.65rem;
		letter-spacing: 0.14em;
		text-transform: uppercase;
	}
	.signal-metrics strong {
		font-size: clamp(1.35rem, 7cqw, 3rem);
		font-weight: 500;
		font-variant-numeric: tabular-nums;
	}

	.fps-lagging {
		color: var(--obs-danger);
	}

	@keyframes signal-title-in {
		from {
			opacity: 0;
			clip-path: inset(0 100% 0 0);
			transform: translateX(-1.5rem);
		}
		to {
			opacity: 1;
			clip-path: inset(0);
			transform: none;
		}
	}

	@keyframes signal-wipe {
		from {
			opacity: 0;
			transform: translateX(-100%);
		}
		45% {
			opacity: 1;
		}
		to {
			opacity: 0;
			transform: translateX(100%);
		}
	}

	@keyframes metrics-in {
		from {
			opacity: 0;
			transform: translateY(0.75rem);
		}
		to {
			opacity: 1;
			transform: none;
		}
	}

	@container (max-width: 520px) {
		.monitor-topbar {
			right: 2rem;
			left: 2rem;
		}
		.monitor-footer {
			right: 1rem;
			left: 1.75rem;
		}
		.brand-full {
			display: none;
		}
		.brand-short {
			display: inline;
		}
		.signal-content {
			top: 44%;
			left: 2.5rem;
		}
		.signal-metrics {
			right: 1.25rem;
			bottom: 6.2rem;
			left: 2.5rem;
		}
		.stop-hint {
			display: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.signal-content,
		.signal-sweep,
		.signal-metrics {
			animation-duration: 1ms;
			animation-delay: 0ms;
		}
		.monitor-signal-band,
		.monitor-signal-band * {
			transition-duration: 1ms;
		}
	}
</style>
