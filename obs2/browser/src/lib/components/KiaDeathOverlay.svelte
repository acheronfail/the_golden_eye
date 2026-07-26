<script lang="ts">
	import { browser } from '$app/environment';
	import { clamp, drawKiaFrame, kiaFadeStartProgress, makeKiaDrips } from '$lib/effects/kiaEffectRenderer';
	import { onDestroy, tick } from 'svelte';

	interface Props {
		trigger: number;
		durationMs?: number;
	}

	let { trigger, durationMs = 2000 }: Props = $props();

	let canvas: HTMLCanvasElement | undefined;
	let playing = $state(false);
	let animationFrame: number | null = null;
	let runId = 0;
	let lastTrigger = 0;

	const resizeCanvas = (): { width: number; height: number } => {
		if (!canvas) return { width: 1, height: 1 };

		const rect = canvas.getBoundingClientRect();
		const width = Math.max(1, Math.round(rect.width));
		const height = Math.max(1, Math.round(rect.height));

		if (canvas.width !== width || canvas.height !== height) {
			canvas.width = width;
			canvas.height = height;
		}

		return { width, height };
	};

	const startAnimation = async (triggerId: number): Promise<void> => {
		if (!browser) return;

		const currentRun = ++runId;
		if (animationFrame !== null) {
			cancelAnimationFrame(animationFrame);
			animationFrame = null;
		}

		playing = true;
		await tick();
		if (currentRun !== runId || !canvas) return;

		const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		const effectiveDuration = reducedMotion ? Math.min(durationMs, 420) : durationMs;
		const fadeStartProgress = kiaFadeStartProgress(effectiveDuration);
		const seed = triggerId * 7919 + 17;
		const drips = makeKiaDrips(seed);
		const start = performance.now();

		const animate = (now: number) => {
			if (currentRun !== runId) return;

			const progress = clamp((now - start) / effectiveDuration, 0, 1);
			const ctx = canvas?.getContext('2d');
			const { width, height } = resizeCanvas();
			const visible =
				ctx &&
				drawKiaFrame(ctx, width, height, {
					progress,
					seed,
					drips,
					reducedMotion,
					fadeStartProgress
				});

			if (progress < 1 && visible) {
				animationFrame = requestAnimationFrame(animate);
				return;
			}

			if (ctx && canvas) {
				ctx.clearRect(0, 0, canvas.width, canvas.height);
			}
			animationFrame = null;
			playing = false;
		};

		animationFrame = requestAnimationFrame(animate);
	};

	$effect(() => {
		const nextTrigger = trigger;
		if (nextTrigger === lastTrigger) return;

		lastTrigger = nextTrigger;
		if (nextTrigger > 0) {
			void startAnimation(nextTrigger);
		}
	});

	onDestroy(() => {
		runId++;
		if (animationFrame !== null) {
			cancelAnimationFrame(animationFrame);
		}
	});
</script>

<canvas
	bind:this={canvas}
	class="pointer-events-none fixed inset-0 z-[60] h-screen w-screen transition-opacity duration-[120ms] ease-out"
	class:opacity-0={!playing}
	class:opacity-100={playing}
	aria-hidden="true"
></canvas>
