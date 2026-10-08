import { expect, it, vi } from 'vitest';
import { drawConfettiFrame, makeConfetti } from './renderer';

const context = () =>
	({
		clearRect: vi.fn(),
		save: vi.fn(),
		restore: vi.fn(),
		translate: vi.fn(),
		rotate: vi.fn(),
		fillRect: vi.fn()
	}) as unknown as CanvasRenderingContext2D;

it('clears the canvas and ends without drawing when the animation completes', () => {
	const ctx = context();
	expect(drawConfettiFrame(ctx, 900, 900, makeConfetti(1), 1, false)).toBe(false);
	expect(ctx.clearRect).toHaveBeenCalledWith(0, 0, 900, 900);
	expect(ctx.fillRect).not.toHaveBeenCalled();
});

it('keeps reduced-motion confetti stationary while normal confetti moves', () => {
	const pieces = makeConfetti(1);
	const first = context();
	const later = context();
	drawConfettiFrame(first, 900, 900, pieces, 0.2, true);
	drawConfettiFrame(later, 900, 900, pieces, 0.7, true);
	expect(vi.mocked(first.translate).mock.calls).toEqual(vi.mocked(later.translate).mock.calls);
	drawConfettiFrame(later, 900, 900, pieces, 0.7, false);
	expect(vi.mocked(first.translate).mock.calls).not.toEqual(vi.mocked(later.translate).mock.calls.slice(180));
});

it('rises smoothly to an early apex without slowing and accelerating again', () => {
	const piece = makeConfetti(1)[0];
	const apex = piece.lift / 7.4 / 2.8;
	const heightAt = (time: number) => {
		const ctx = context();
		drawConfettiFrame(ctx, 900, 900, [piece], piece.delay + time, false);
		return vi.mocked(ctx.translate).mock.calls[0][1];
	};
	let previousRise = Infinity;
	for (let step = 1; step <= 20; step++) {
		const rise = heightAt((apex * (step - 1)) / 20) - heightAt((apex * step) / 20);
		expect(rise).toBeGreaterThan(0);
		expect(rise).toBeLessThan(previousRise);
		previousRise = rise;
	}
	expect(heightAt(apex + 0.02)).toBeGreaterThan(heightAt(apex));
	expect(heightAt(apex + 0.1) - heightAt(apex)).toBeCloseTo(900 * 3.7 * 0.1 ** 2);
});
