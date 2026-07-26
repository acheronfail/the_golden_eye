export interface KiaDrip {
	x: number;
	radius: number;
	depth: number;
	phase: number;
	wobble: number;
}

export interface KiaFrame {
	progress: number;
	seed: number;
	drips: KiaDrip[];
	reducedMotion: boolean;
	fadeStartProgress: number;
}

export const KIA_QUICK_FADE_MS = 250;

export const clamp = (value: number, min: number, max: number): number => Math.min(max, Math.max(min, value));

const lerp = (a: number, b: number, t: number): number => a + (b - a) * t;

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
	let amplitude = 0.55;
	let frequency = 1;
	let normalization = 0;

	for (let octave = 0; octave < 4; octave++) {
		sum += valueNoise(x * frequency, seed + octave * 17) * amplitude;
		normalization += amplitude;
		amplitude *= 0.5;
		frequency *= 2.03;
	}

	return sum / normalization;
};

export const makeKiaDrips = (seed: number): KiaDrip[] => {
	const count = 2;
	return Array.from({ length: count }, (_, index) => ({
		x: (index + hash(index * 13.37, seed)) / count,
		radius: 0.095 + hash(index * 19.91 + 1, seed) * 0.16,
		depth: 0.05 + hash(index * 23.17 + 2, seed) * 0.1,
		phase: hash(index * 29.03 + 3, seed) * Math.PI * 2,
		wobble: 3.6 + hash(index * 31.71 + 4, seed) * 4.5
	}));
};

export const kiaFadeStartProgress = (durationMs: number): number =>
	1 - clamp(KIA_QUICK_FADE_MS / durationMs, 0.01, 0.2);

const edgeY = (x: number, width: number, height: number, baseY: number, frame: KiaFrame): number => {
	const xNorm = width <= 0 ? 0 : x / width;
	const waveHeight = height * (frame.reducedMotion ? 0.01 : 0.115);
	const drift = frame.reducedMotion ? 0 : frame.progress * 1.35;
	const broadWave = fbm(xNorm * 2.35 + drift * 0.42, frame.seed);
	const fineWave = fbm(xNorm * 7.5 - drift * 1.05, frame.seed + 23) * 0.48;
	let y = baseY + (broadWave + fineWave) * waveHeight;

	const dripGrowth = smootherStep(0.02, 0.45, frame.progress);
	for (const drip of frame.drips) {
		const center = drip.x + Math.sin(frame.progress * drip.wobble + drip.phase) * 0.028;
		const dx = xNorm - center;
		const blob = Math.exp(-(dx * dx) / (2 * drip.radius * drip.radius));
		const wobble = 0.66 + Math.sin(frame.progress * drip.wobble + drip.phase) * 0.34;
		y += drip.depth * height * blob * dripGrowth * wobble;
	}

	return y;
};

const drawEdge = (
	ctx: CanvasRenderingContext2D,
	width: number,
	height: number,
	baseY: number,
	frame: KiaFrame,
	step: number
): void => {
	ctx.beginPath();
	ctx.moveTo(0, edgeY(0, width, height, baseY, frame));
	for (let x = step; x < width; x += step) {
		ctx.lineTo(x, edgeY(x, width, height, baseY, frame));
	}
	ctx.lineTo(width, edgeY(width, width, height, baseY, frame));
};

export const drawKiaFrame = (
	ctx: CanvasRenderingContext2D,
	width: number,
	height: number,
	frame: KiaFrame
): boolean => {
	const slideProgress = clamp(frame.progress / frame.fadeStartProgress, 0, 1);
	const baseY = height * 1.1 * slideProgress;
	const fade =
		frame.progress < frame.fadeStartProgress ? 1 : 1 - smootherStep(frame.fadeStartProgress, 1, frame.progress);
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
	ctx.lineTo(width, edgeY(width, width, height, baseY, frame));
	for (let x = width; x > 0; x -= step) {
		ctx.lineTo(x, edgeY(x, width, height, baseY, frame));
	}
	ctx.lineTo(0, edgeY(0, width, height, baseY, frame));
	ctx.closePath();
	ctx.fillStyle = fill;
	ctx.fill();

	drawEdge(ctx, width, height, baseY, frame, step);
	ctx.lineWidth = Math.max(2, height * 0.006);
	ctx.strokeStyle = 'rgba(55, 0, 0, 0.38)';
	ctx.stroke();

	drawEdge(ctx, width, height, baseY - height * 0.004, frame, step);
	ctx.lineWidth = Math.max(1, height * 0.0025);
	ctx.strokeStyle = 'rgba(255, 58, 58, 0.16)';
	ctx.stroke();

	ctx.restore();
	return true;
};
