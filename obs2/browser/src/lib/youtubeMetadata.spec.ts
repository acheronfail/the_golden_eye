import { describe, expect, it } from 'vitest';
import type { RunClip } from './api';
import { datetimeLocalForClip, formatDatetimeLocal } from './youtubeMetadata';

const clip = (timestamp = '2026-07-18T10:30:45Z'): RunClip => ({
	path: '/runs/dam.mov',
	fileName: 'dam.mov',
	directory: '/runs',
	sizeBytes: 1024,
	modified: timestamp,
	durationSecs: 70,
	metadata: {
		timestamp,
		time: '01:23',
		timeSeconds: 83,
		level: 'Dam',
		levelNumber: 1,
		difficulty: 'Agent',
		status: 'complete',
		romLanguage: 'en',
		sourceName: 'N64 Capture',
		comment: 'Created by test',
		pluginVersion: '1.2.3'
	}
});

describe('YouTube datetime local helper', () => {
	it('formats datetime_local with the browser locale', () => {
		const timestamp = '2026-07-18T10:30:45Z';

		expect(formatDatetimeLocal(timestamp, 'en-US')).toBe(new Date(timestamp).toLocaleString('en-US'));
		expect(datetimeLocalForClip(clip(timestamp), 'en-US')).toBe(new Date(timestamp).toLocaleString('en-US'));
	});

	it('falls back to the raw timestamp when the timestamp is invalid', () => {
		expect(formatDatetimeLocal('not a timestamp', 'en-US')).toBe('not a timestamp');
		expect(datetimeLocalForClip(clip('not a timestamp'), 'en-US')).toBe('not a timestamp');
	});
});
