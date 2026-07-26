import { describe, expect, it, vi } from 'vitest';
import { drawKiaFrame, kiaFadeStartProgress, makeKiaDrips, type KiaFrame } from './kiaEffectRenderer';

const makeContext = (): CanvasRenderingContext2D => {
	const gradient = { addColorStop: vi.fn() };
	return {
		beginPath: vi.fn(),
		clearRect: vi.fn(),
		closePath: vi.fn(),
		createLinearGradient: vi.fn(() => gradient),
		fill: vi.fn(),
		lineTo: vi.fn(),
		moveTo: vi.fn(),
		restore: vi.fn(),
		save: vi.fn(),
		stroke: vi.fn()
	} as unknown as CanvasRenderingContext2D;
};

const makeFrame = (progress: number): KiaFrame => ({
	progress,
	seed: 7919,
	drips: makeKiaDrips(7919),
	reducedMotion: false,
	fadeStartProgress: kiaFadeStartProgress(2000)
});

describe('KIA effect renderer', () => {
	it('creates stable drips from the animation seed', () => {
		expect(makeKiaDrips(42)).toEqual(makeKiaDrips(42));
		expect(makeKiaDrips(42)).not.toEqual(makeKiaDrips(43));
	});

	it('limits the fade to at most the final fifth of the animation', () => {
		expect(kiaFadeStartProgress(2000)).toBe(0.875);
		expect(kiaFadeStartProgress(400)).toBe(0.8);
	});

	it('draws visible frames', () => {
		const context = makeContext();

		expect(drawKiaFrame(context, 640, 360, makeFrame(0.5))).toBe(true);
		expect(context.fill).toHaveBeenCalledOnce();
		expect(context.stroke).toHaveBeenCalledTimes(2);
		expect(context.restore).toHaveBeenCalledOnce();
	});

	it('clears without drawing once the fade completes', () => {
		const context = makeContext();

		expect(drawKiaFrame(context, 640, 360, makeFrame(1))).toBe(false);
		expect(context.clearRect).toHaveBeenCalledWith(0, 0, 640, 360);
		expect(context.save).not.toHaveBeenCalled();
	});
});
