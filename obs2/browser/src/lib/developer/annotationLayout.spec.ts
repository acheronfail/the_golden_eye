import { describe, expect, it } from 'vitest';
import type { AnnotationSet } from '$lib/api';
import { annotationListItems, placeAnnotations } from './annotationLayout';

const set: AnnotationSet = {
	id: 'digits',
	label: 'Time digits',
	annotations: [
		{ label: 'minutes tens', x: -5, y: 20, w: 30, h: 40, score: 0.987 },
		{ label: 'seconds ones', x: 310, y: 140, w: 30, h: 40 }
	]
};

describe('developer annotation layout', () => {
	it('builds stable labels and colors from an annotation set', () => {
		const items = annotationListItems(set);

		expect(items.map((item) => [item.id, item.label])).toEqual([
			['digits:0', 'minutes tens 0.99'],
			['digits:1', 'seconds ones']
		]);
		expect(items[0].color).not.toBe(items[1].color);
	});

	it('clips regions and labels to the frame', () => {
		const placed = placeAnnotations(annotationListItems(set), 320, 180);

		expect(placed[0].region).toMatchObject({ x: 0, y: 20, w: 30, h: 40 });
		expect(placed[1].region.x + placed[1].region.w).toBeLessThanOrEqual(320);
		for (const item of placed) {
			expect(item.label.x).toBeGreaterThanOrEqual(0);
			expect(item.label.y).toBeGreaterThanOrEqual(0);
			expect(item.label.x + item.label.w).toBeLessThanOrEqual(320);
			expect(item.label.y + item.label.h).toBeLessThanOrEqual(180);
		}
	});

	it('returns no layout before frame dimensions are known', () => {
		expect(placeAnnotations(annotationListItems(set), 0, 100)).toEqual([]);
	});
});
