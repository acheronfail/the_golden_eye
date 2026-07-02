<script lang="ts">
	import { browser } from '$app/environment';
	import { onDestroy, tick } from 'svelte';

	interface Props {
		trigger: number;
		durationMs?: number;
	}

	interface Drip {
		x: number;
		radius: number;
		depth: number;
		phase: number;
		wobble: number;
	}

	let { trigger, durationMs = 2000 }: Props = $props();

	let canvas: HTMLCanvasElement | undefined;
	let playing = $state(false);
	let animationFrame: number | null = null;
	let runId = 0;
	let lastTrigger = 0;

	const clamp = (value: number, min: number, max: number): number => Math.min(max, Math.max(min, value));
	const lerp = (a: number, b: number, t: number): number => a + (b - a) * t;
	const QUICK_FADE_MS = 250;

	const smootherStep = (edge0: number, edge1: number, value: number): number => {
		const x = clamp((value - edge0) / (edge1 - edge0), 0, 1);
		return x * x * x * (x * (x * 6 - 15) + 10);
	};

	const hash = (n: number, seed: number): number => {
		const value = Math.sin(n * 127.1 + seed * 311.7) * 43758.5453123;
		return value - Math.floor(value);
	};

	const valueNoise = (x: number, seed: number): number => {
		const i = Math.floor(x);
		const f = x - i;
		const u = f * f * (3 - 2 * f);
		return lerp(hash(i, seed), hash(i + 1, seed), u) * 2 - 1;
	};

	const fbm = (x: number, seed: number): number => {
		let sum = 0;
		let amp = 0.55;
		let freq = 1;
		let norm = 0;

		for (let octave = 0; octave < 4; octave++) {
			sum += valueNoise(x * freq, seed + octave * 17) * amp;
			norm += amp;
			amp *= 0.5;
			freq *= 2.03;
		}

		return sum / norm;
	};

	const makeDrips = (seed: number): Drip[] => {
		const count = 2;
		return Array.from({ length: count }, (_, i) => ({
			x: (i + hash(i * 13.37, seed)) / count,
			radius: 0.095 + hash(i * 19.91 + 1, seed) * 0.16,
			depth: 0.05 + hash(i * 23.17 + 2, seed) * 0.1,
			phase: hash(i * 29.03 + 3, seed) * Math.PI * 2,
			wobble: 3.6 + hash(i * 31.71 + 4, seed) * 4.5
		}));
	};

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

	const edgeY = (
		x: number,
		width: number,
		height: number,
		baseY: number,
		progress: number,
		seed: number,
		drips: Drip[],
		reducedMotion: boolean
	): number => {
		const xNorm = width <= 0 ? 0 : x / width;
		const waveHeight = height * (reducedMotion ? 0.01 : 0.115);
		const drift = reducedMotion ? 0 : progress * 1.35;
		const broadWave = fbm(xNorm * 2.35 + drift * 0.42, seed);
		const fineWave = fbm(xNorm * 7.5 - drift * 1.05, seed + 23) * 0.48;
		let y = baseY + (broadWave + fineWave) * waveHeight;

		const dripGrowth = smootherStep(0.02, 0.45, progress);
		for (const drip of drips) {
			const center = drip.x + Math.sin(progress * drip.wobble + drip.phase) * 0.028;
			const dx = xNorm - center;
			const blob = Math.exp(-(dx * dx) / (2 * drip.radius * drip.radius));
			const wobble = 0.66 + Math.sin(progress * drip.wobble + drip.phase) * 0.34;
			y += drip.depth * height * blob * dripGrowth * wobble;
		}

		return y;
	};

	const drawEdge = (
		ctx: CanvasRenderingContext2D,
		width: number,
		height: number,
		baseY: number,
		progress: number,
		seed: number,
		drips: Drip[],
		reducedMotion: boolean,
		step: number
	): void => {
		ctx.beginPath();
		ctx.moveTo(0, edgeY(0, width, height, baseY, progress, seed, drips, reducedMotion));
		for (let x = step; x < width; x += step) {
			ctx.lineTo(x, edgeY(x, width, height, baseY, progress, seed, drips, reducedMotion));
		}
		ctx.lineTo(width, edgeY(width, width, height, baseY, progress, seed, drips, reducedMotion));
	};

	const drawFrame = (
		progress: number,
		seed: number,
		drips: Drip[],
		reducedMotion: boolean,
		fadeStartProgress: number
	): boolean => {
		if (!canvas) return false;
		const ctx = canvas.getContext('2d');
		if (!ctx) return false;

		const { width, height } = resizeCanvas();
		const slideProgress = clamp(progress / fadeStartProgress, 0, 1);
		const baseY = height * 1.1 * slideProgress;
		const fade = progress < fadeStartProgress ? 1 : 1 - smootherStep(fadeStartProgress, 1, progress);
		const step = Math.max(2, Math.round(width / 240));

		ctx.clearRect(0, 0, width, height);
		if (fade <= 0) return false;

		ctx.save();
		ctx.globalAlpha = fade;

		const fill = ctx.createLinearGradient(0, 0, 0, height);
		fill.addColorStop(0, 'rgba(236, 18, 18, 0.42)');
		fill.addColorStop(0.55, 'rgba(166, 0, 0, 0.58)');
		fill.addColorStop(1, 'rgba(78, 0, 0, 0.66)');

		ctx.beginPath();
		ctx.moveTo(0, 0);
		ctx.lineTo(width, 0);
		ctx.lineTo(width, edgeY(width, width, height, baseY, progress, seed, drips, reducedMotion));
		for (let x = width; x > 0; x -= step) {
			ctx.lineTo(x, edgeY(x, width, height, baseY, progress, seed, drips, reducedMotion));
		}
		ctx.lineTo(0, edgeY(0, width, height, baseY, progress, seed, drips, reducedMotion));
		ctx.closePath();
		ctx.fillStyle = fill;
		ctx.fill();

		drawEdge(ctx, width, height, baseY, progress, seed, drips, reducedMotion, step);
		ctx.lineWidth = Math.max(2, height * 0.006);
		ctx.strokeStyle = 'rgba(55, 0, 0, 0.38)';
		ctx.stroke();

		drawEdge(ctx, width, height, baseY - height * 0.004, progress, seed, drips, reducedMotion, step);
		ctx.lineWidth = Math.max(1, height * 0.0025);
		ctx.strokeStyle = 'rgba(255, 58, 58, 0.16)';
		ctx.stroke();

		ctx.restore();
		return true;
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
		const fadeStartProgress = 1 - clamp(QUICK_FADE_MS / effectiveDuration, 0.01, 0.2);
		const seed = triggerId * 7919 + 17;
		const drips = makeDrips(seed);
		const start = performance.now();

		const animate = (now: number) => {
			if (currentRun !== runId) return;

			const progress = clamp((now - start) / effectiveDuration, 0, 1);
			const visible = drawFrame(progress, seed, drips, reducedMotion, fadeStartProgress);

			if (progress < 1 && visible) {
				animationFrame = requestAnimationFrame(animate);
				return;
			}

			const ctx = canvas?.getContext('2d');
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

<canvas bind:this={canvas} class={`kia-death-overlay${playing ? ' kia-death-overlay-visible' : ''}`} aria-hidden="true"
></canvas>

<style>
	.kia-death-overlay {
		position: fixed;
		inset: 0;
		z-index: 60;
		width: 100vw;
		height: 100vh;
		pointer-events: none;
		opacity: 0;
		transition: opacity 120ms ease-out;
	}

	.kia-death-overlay-visible {
		opacity: 1;
	}
</style>
