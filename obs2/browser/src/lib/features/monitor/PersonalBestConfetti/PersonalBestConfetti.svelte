<script lang="ts">
	import { browser } from '$app/environment';
	import { drawConfettiFrame, makeConfetti } from './renderer';
	import { onDestroy, tick } from 'svelte';

	interface Props {
		trigger: number;
		durationMs?: number;
		reducedMotion?: boolean;
	}

	let { trigger, durationMs = 4000, reducedMotion }: Props = $props();

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

		const reduceMotion = reducedMotion ?? window.matchMedia('(prefers-reduced-motion: reduce)').matches;
		const effectiveDuration = reduceMotion ? 420 : durationMs;
		const seed = triggerId * 7919 + 17;
		const pieces = makeConfetti(seed);
		const start = performance.now();

		const animate = (now: number) => {
			if (currentRun !== runId) return;

			const progress = Math.min(1, Math.max(0, (now - start) / Math.max(1, effectiveDuration)));
			const ctx = canvas?.getContext('2d');
			const { width, height } = resizeCanvas();
			const visible = ctx && drawConfettiFrame(ctx, width, height, pieces, progress, reduceMotion);

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
