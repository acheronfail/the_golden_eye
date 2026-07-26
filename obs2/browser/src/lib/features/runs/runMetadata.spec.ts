import { describe, expect, it } from 'vitest';
import type { RunClip } from '$lib/api';
import { metadataDraftFromClip, normalizeRunTimeInput, runBrowserLabels, sameMetadataDraft } from './runMetadata';

const clip = {
	runId: 'run-1',
	path: '/runs/clip.mp4',
	fileName: 'clip.mp4',
	directory: '/runs',
	metadata: {
		gameLanguage: 'en',
		romVersion: 'ntsc-u',
		status: 'complete',
		difficulty: 'Agent',
		time: '1:05',
		level: 'Dam',
		timestamp: '2026-01-01T00:00:00Z'
	}
} as RunClip;

describe('run metadata', () => {
	it('creates a validated editable draft', () => {
		const draft = metadataDraftFromClip(clip);

		expect(draft).toEqual({
			gameLanguage: 'en',
			romVersion: 'ntsc-u',
			status: 'complete',
			difficulty: 'Agent',
			time: '1:05',
			level: 'Dam'
		});
		expect(sameMetadataDraft(draft, { ...draft })).toBe(true);
	});

	it.each([
		['1:05', '01:05'],
		['12:9', '12:09'],
		['12:60', '12:60'],
		['bad', 'bad']
	])('normalizes %s to %s', (input, output) => {
		expect(normalizeRunTimeInput(input)).toBe(output);
	});

	it('uses platform-specific file-browser labels', () => {
		expect(runBrowserLabels('MacIntel')).toEqual({ file: 'Show in Finder', folder: 'show clips in finder' });
		expect(runBrowserLabels('Win32').file).toBe('Show in Explorer');
	});
});
