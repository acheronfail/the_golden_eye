<script lang="ts">
	import type { MonitorViewProps } from './monitorView';
	import { formatMonitorTime, monitorPresentation } from './monitorView';
	import RecentRuns from './RecentRuns.svelte';

	let {
		verified,
		monitoring,
		transition = null,
		recordingState = null,
		match = null,
		fps = null,
		showMonitorFps = false,
		recentRuns = [],
		recentRunsBusyId = null,
		recentRunsError = null,
		onKeepRun = () => {},
		onStop
	}: MonitorViewProps = $props();

	const presentation = $derived(
		monitorPresentation({ verified, monitoring, transition, recordingState, match, fps, showMonitorFps, onStop })
	);
</script>

<main
	class="monitor-mission-glass"
	data-phase={presentation.phase}
	aria-busy={presentation.waitingForObs || !verified}
	aria-live="polite"
>
	<div class="glass-atmosphere" aria-hidden="true"></div>
	<div class="glass-frame" aria-hidden="true"></div>

	<header class="monitor-topbar">
		<span class="monitor-brand"><span class="brand-light" aria-hidden="true"></span> LIVE MONITOR</span>
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
		<div class="reticle-motion" aria-hidden="true">
			<div class="glass-reticle"><span></span></div>
		</div>
	{/key}

	<div class="glass-layout">
		{#key presentation.animationKey}
			<section class="glass-panel">
				<p class="glass-kicker">{verified ? presentation.statusLabel : 'Verifying source'}</p>
				<h1>{verified ? presentation.title : 'checking source'}</h1>
				<p
					class="glass-detail"
					class:invisible={!presentation.showDetail || !verified}
					aria-hidden={!presentation.showDetail || !verified}
				>
					{verified ? presentation.detail : '...'}
				</p>

				{#if match?.times && !presentation.waitingForObs}
					<div class="glass-metrics" aria-label="Run times">
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
			</section>
		{/key}

		<RecentRuns
			variant="mission-glass"
			runs={recentRuns}
			busyRunId={recentRunsBusyId}
			error={recentRunsError}
			onKeep={onKeepRun}
		/>
	</div>

	<footer class="monitor-footer">
		<span>{presentation.phase}</span>
		{#if monitoring}
			<span class="stop-hint">escape or space to stop</span>
		{/if}
		{#if monitoring && showMonitorFps && presentation.fpsText}
			<span class:text-amber-400={presentation.fpsWarning} class:text-(--obs-danger)={presentation.fpsLagging}>
				{presentation.fpsText}
			</span>
		{/if}
	</footer>
</main>

<style>
	.monitor-mission-glass {
		--monitor-accent: var(--obs-monitor-waiting);
		--monitor-surface: var(--obs-monitor-waiting-surface);
		position: relative;
		isolation: isolate;
		display: flex;
		height: 100%;
		min-height: 0;
		container-type: size;
		align-items: center;
		justify-content: center;
		overflow: hidden;
		background: var(--obs-bg);
		color: var(--obs-text);
	}

	.monitor-mission-glass[data-phase='recording'] {
		--monitor-accent: var(--obs-success);
		--monitor-surface: var(--obs-success-surface);
	}

	.monitor-mission-glass[data-phase='complete'] {
		--monitor-accent: var(--obs-gold-hover);
		--monitor-surface: var(--obs-gold-surface);
	}

	.monitor-mission-glass[data-phase='danger'] {
		--monitor-accent: var(--obs-danger);
		--monitor-surface: var(--obs-danger-surface);
	}

	.monitor-mission-glass[data-phase='neutral'] {
		--monitor-accent: var(--obs-text-muted);
		--monitor-surface: rgb(182 186 196 / 11%);
	}

	.glass-atmosphere {
		position: absolute;
		inset: 0;
		z-index: -4;
		background:
			radial-gradient(circle at 50% 48%, var(--monitor-surface), transparent 42%),
			linear-gradient(145deg, var(--monitor-surface), transparent 38%);
		transition: background 240ms ease;
	}

	.glass-atmosphere::after {
		position: absolute;
		inset: 0;
		background-image:
			linear-gradient(rgb(255 255 255 / 2%) 1px, transparent 1px),
			linear-gradient(90deg, rgb(255 255 255 / 2%) 1px, transparent 1px);
		background-size: 3rem 3rem;
		content: '';
		mask-image: radial-gradient(circle, #000, transparent 72%);
	}

	.glass-frame {
		position: absolute;
		inset: 1rem;
		z-index: -1;
		border: 1px solid color-mix(in srgb, var(--monitor-accent) 50%, transparent);
		border-radius: 0.8rem;
		box-shadow: inset 0 0 2.5rem var(--monitor-surface);
		transition:
			border-color 240ms ease,
			box-shadow 240ms ease;
	}

	.monitor-topbar,
	.monitor-footer {
		position: absolute;
		right: 1.5rem;
		left: 1.5rem;
		z-index: 5;
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
		left: 2rem;
		color: var(--obs-text-dim);
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

	.reticle-motion {
		position: absolute;
		z-index: -2;
		width: min(78cqw, 34rem);
		aspect-ratio: 1;
		animation: reticle-in 560ms cubic-bezier(0.2, 0.82, 0.2, 1) both;
	}

	.glass-layout {
		position: relative;
		z-index: 2;
		display: grid;
		width: min(82cqw, 42rem);
		max-height: calc(100cqh - 9rem);
		gap: clamp(0.65rem, 2cqh, 1rem);
	}

	.glass-reticle {
		position: absolute;
		inset: 0;
		border: 1px solid color-mix(in srgb, var(--monitor-accent) 58%, transparent);
		border-radius: 50%;
		box-shadow: 0 0 5rem var(--monitor-surface);
		transition:
			border-color 240ms ease,
			box-shadow 240ms ease;
	}

	.glass-reticle::before,
	.glass-reticle::after,
	.glass-reticle span {
		position: absolute;
		content: '';
	}

	.glass-reticle::before {
		top: 50%;
		right: -13%;
		left: -13%;
		height: 1px;
		background: color-mix(in srgb, var(--monitor-accent) 36%, transparent);
	}

	.glass-reticle::after {
		top: -13%;
		bottom: -13%;
		left: 50%;
		width: 1px;
		background: color-mix(in srgb, var(--monitor-accent) 36%, transparent);
	}

	.glass-reticle span {
		inset: 14%;
		border: 1px dashed color-mix(in srgb, var(--monitor-accent) 27%, transparent);
		border-radius: 50%;
	}

	.glass-panel {
		position: relative;
		width: 100%;
		padding: clamp(1.25rem, 4.5cqw, 2.5rem);
		border: 1px solid color-mix(in srgb, var(--monitor-accent) 38%, var(--obs-border-soft));
		border-radius: clamp(1rem, 4cqw, 1.6rem);
		background: rgb(37 41 52 / 90%);
		box-shadow:
			0 1.5rem 5rem rgb(0 0 0 / 35%),
			0 0 4rem var(--monitor-surface),
			inset 0 1px 0 rgb(255 255 255 / 11%);
		text-align: center;
		animation: glass-panel-in 520ms cubic-bezier(0.2, 0.82, 0.2, 1) both;
		transition:
			border-color 240ms ease,
			box-shadow 240ms ease;
	}

	.glass-kicker,
	.glass-detail {
		font-family: var(--font-mono, ui-monospace, monospace);
		font-size: clamp(0.65rem, 2.8cqw, 0.82rem);
		letter-spacing: 0.15em;
		text-transform: uppercase;
	}

	.glass-kicker {
		color: var(--monitor-accent);
		transition: color 240ms ease;
	}

	h1 {
		margin: 0.55rem 0 0.7rem;
		color: color-mix(in srgb, var(--monitor-accent) 12%, var(--obs-text));
		font-size: clamp(2.25rem, 11cqw, 5rem);
		font-weight: 600;
		line-height: 0.92;
		letter-spacing: -0.065em;
		overflow-wrap: anywhere;
		transition: color 240ms ease;
	}

	.glass-detail {
		color: var(--obs-text-dim);
	}

	.glass-metrics {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: clamp(0.7rem, 4cqw, 2rem);
		margin-top: clamp(1.5rem, 5cqw, 2.5rem);
		padding-top: clamp(1.25rem, 4cqw, 2rem);
		border-top: 1px solid color-mix(in srgb, var(--monitor-accent) 25%, var(--obs-border-muted));
		font-family: var(--font-mono, ui-monospace, monospace);
	}

	.glass-metrics span {
		display: grid;
		gap: 0.2rem;
		min-width: 0;
	}
	.glass-metrics small {
		color: var(--obs-text-dim);
		font-size: 0.65rem;
		letter-spacing: 0.12em;
		text-transform: uppercase;
	}
	.glass-metrics strong {
		font-size: clamp(1.25rem, 6cqw, 2.6rem);
		font-weight: 500;
		font-variant-numeric: tabular-nums;
	}

	@keyframes glass-panel-in {
		from {
			opacity: 0;
			transform: translateY(1.2rem) scale(0.96);
			filter: blur(0.35rem);
		}
		to {
			opacity: 1;
			transform: none;
			filter: blur(0);
		}
	}

	@keyframes reticle-in {
		from {
			opacity: 0;
			transform: scale(0.62);
		}
		70% {
			opacity: 1;
		}
		to {
			opacity: 1;
			transform: scale(1);
		}
	}

	@container (max-width: 520px) {
		.monitor-topbar {
			right: 2rem;
			left: 2rem;
		}
		.monitor-footer {
			right: 1.1rem;
			left: 2rem;
		}
		.glass-panel {
			padding: 1.35rem;
		}
		.glass-layout {
			width: calc(100cqw - 3rem);
		}
		.reticle-motion {
			width: 92cqw;
		}
		.stop-hint {
			display: none;
		}
	}

	@media (prefers-reduced-motion: reduce) {
		.glass-panel,
		.reticle-motion {
			animation-duration: 1ms;
		}
		.monitor-mission-glass,
		.monitor-mission-glass * {
			transition-duration: 1ms;
		}
	}
</style>
