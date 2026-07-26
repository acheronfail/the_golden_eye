import type { AnnotationRect, AnnotationSet } from '$lib/api';

export const LABEL_PADDING_X = 8;
export const LABEL_PADDING_Y = 5;
export const LABEL_LINE_HEIGHT = 18;
const LABEL_CHAR_WIDTH = 8.3;
const LABEL_MAX_CHARS = 28;
const LABEL_MARGIN = 6;

const ANNOTATION_COLORS = [
	'#22d3ee',
	'#fbbf24',
	'#fb7185',
	'#a78bfa',
	'#34d399',
	'#f97316',
	'#60a5fa',
	'#f472b6',
	'#bef264',
	'#2dd4bf'
];

export interface OverlayBox {
	x: number;
	y: number;
	w: number;
	h: number;
}

export interface OverlayPoint {
	x: number;
	y: number;
}

export interface AnnotationListItem {
	id: string;
	index: number;
	annotation: AnnotationRect;
	label: string;
	color: string;
	fill: string;
}

export interface PlacedAnnotation {
	index: number;
	id: string;
	region: OverlayBox;
	label: OverlayBox;
	lines: string[];
	color: string;
	fill: string;
	connectorStart: OverlayPoint;
	connectorEnd: OverlayPoint;
}

const clamp = (value: number, min: number, max: number) => Math.min(Math.max(value, min), max);
const annotationColor = (index: number) => ANNOTATION_COLORS[index % ANNOTATION_COLORS.length];
const annotationFill = (color: string) => `${color}26`;
const annotationId = (set: AnnotationSet, index: number) => `${set.id}:${index}`;
const annotationText = (annotation: AnnotationRect) =>
	annotation.score == null ? annotation.label : `${annotation.label} ${annotation.score.toFixed(2)}`;

export function annotationListItems(set: AnnotationSet | null): AnnotationListItem[] {
	return (
		set?.annotations.map((annotation, index) => {
			const color = annotationColor(index);
			return {
				id: annotationId(set, index),
				index,
				annotation,
				label: annotationText(annotation),
				color,
				fill: annotationFill(color)
			};
		}) ?? []
	);
}

function normalizeRegion(annotation: AnnotationRect, frameWidth: number, frameHeight: number): OverlayBox {
	const frameW = Math.max(1, frameWidth);
	const frameH = Math.max(1, frameHeight);
	const x = clamp(annotation.x, 0, frameW - 1);
	const y = clamp(annotation.y, 0, frameH - 1);
	return {
		x,
		y,
		w: clamp(annotation.w, 1, frameW - x),
		h: clamp(annotation.h, 1, frameH - y)
	};
}

function wrapLabel(text: string): string[] {
	const words = text.split(/\s+/).filter(Boolean);
	const lines: string[] = [];
	let line = '';

	for (const word of words) {
		const next = line ? `${line} ${word}` : word;
		if (next.length <= LABEL_MAX_CHARS) {
			line = next;
			continue;
		}
		if (line) lines.push(line);
		if (word.length <= LABEL_MAX_CHARS) {
			line = word;
		} else {
			lines.push(word.slice(0, LABEL_MAX_CHARS - 1));
			line = word.slice(LABEL_MAX_CHARS - 1);
		}
	}
	if (line) lines.push(line);
	return lines.length ? lines : [text];
}

function labelSize(lines: string[]): { w: number; h: number } {
	return {
		w: Math.max(56, Math.ceil(Math.max(...lines.map((line) => line.length)) * LABEL_CHAR_WIDTH + LABEL_PADDING_X * 2)),
		h: Math.ceil(lines.length * LABEL_LINE_HEIGHT + LABEL_PADDING_Y * 2)
	};
}

function clampLabel(box: OverlayBox, frameWidth: number, frameHeight: number): OverlayBox {
	return {
		...box,
		x: clamp(box.x, LABEL_MARGIN, Math.max(LABEL_MARGIN, frameWidth - box.w - LABEL_MARGIN)),
		y: clamp(box.y, LABEL_MARGIN, Math.max(LABEL_MARGIN, frameHeight - box.h - LABEL_MARGIN))
	};
}

function boxCenter(box: OverlayBox): OverlayPoint {
	return { x: box.x + box.w / 2, y: box.y + box.h / 2 };
}

function overlapArea(a: OverlayBox, b: OverlayBox): number {
	const x = Math.max(0, Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x));
	const y = Math.max(0, Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y));
	return x * y;
}

function distanceSquared(a: OverlayPoint, b: OverlayPoint): number {
	const dx = a.x - b.x;
	const dy = a.y - b.y;
	return dx * dx + dy * dy;
}

function connectorStart(label: OverlayBox, target: OverlayPoint): OverlayPoint {
	return {
		x: clamp(target.x, label.x, label.x + label.w),
		y: clamp(target.y, label.y, label.y + label.h)
	};
}

function labelCandidates(
	region: OverlayBox,
	label: { w: number; h: number },
	frameWidth: number,
	frameHeight: number
): OverlayBox[] {
	const center = boxCenter(region);
	const candidates: OverlayBox[] = [];
	const offsets = [10, 34, 66, 104, 146];

	for (const offset of offsets) {
		candidates.push({ x: center.x - label.w / 2, y: region.y - label.h - offset, w: label.w, h: label.h });
		candidates.push({ x: center.x - label.w / 2, y: region.y + region.h + offset, w: label.w, h: label.h });
		candidates.push({ x: region.x + region.w + offset, y: center.y - label.h / 2, w: label.w, h: label.h });
		candidates.push({ x: region.x - label.w - offset, y: center.y - label.h / 2, w: label.w, h: label.h });
		candidates.push({ x: region.x + region.w + offset, y: region.y - label.h - offset, w: label.w, h: label.h });
		candidates.push({ x: region.x - label.w - offset, y: region.y - label.h - offset, w: label.w, h: label.h });
		candidates.push({ x: region.x + region.w + offset, y: region.y + region.h + offset, w: label.w, h: label.h });
		candidates.push({ x: region.x - label.w - offset, y: region.y + region.h + offset, w: label.w, h: label.h });
	}

	const laneYStep = label.h + 5;
	const laneXs = [LABEL_MARGIN, frameWidth - label.w - LABEL_MARGIN, frameWidth / 2 - label.w / 2];
	for (const x of laneXs) {
		for (let y = LABEL_MARGIN; y <= frameHeight - label.h - LABEL_MARGIN; y += laneYStep) {
			candidates.push({ x, y, w: label.w, h: label.h });
		}
	}
	return candidates.map((candidate) => clampLabel(candidate, frameWidth, frameHeight));
}

export function placeAnnotations(
	items: AnnotationListItem[],
	frameWidth: number,
	frameHeight: number
): PlacedAnnotation[] {
	if (frameWidth <= 0 || frameHeight <= 0) return [];

	const regions = items.map((item) => normalizeRegion(item.annotation, frameWidth, frameHeight));
	const occupied: OverlayBox[] = [];
	return items.map((item, itemIndex) => {
		const { index, id, label: labelText, color, fill } = item;
		const region = regions[itemIndex];
		const lines = wrapLabel(labelText);
		const size = labelSize(lines);
		const target = boxCenter(region);
		const label = labelCandidates(region, size, frameWidth, frameHeight)
			.map((candidate) => {
				const labelOverlap = occupied.reduce((sum, box) => sum + overlapArea(candidate, box), 0);
				const regionOverlap = regions.reduce((sum, box) => sum + overlapArea(candidate, box), 0);
				return {
					box: candidate,
					score:
						labelOverlap * 100_000 +
						overlapArea(candidate, region) * 1_000 +
						regionOverlap * 25 +
						distanceSquared(boxCenter(candidate), target)
				};
			})
			.sort((a, b) => a.score - b.score)[0].box;

		occupied.push(label);
		return {
			index,
			id,
			region,
			label,
			lines,
			color,
			fill,
			connectorStart: connectorStart(label, target),
			connectorEnd: target
		};
	});
}
