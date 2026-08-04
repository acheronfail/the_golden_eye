import { afterEach, describe, expect, it, vi } from 'vitest';
import type { RunClip } from '$lib/api';
import { datetimeLocalForClip, formatDatetimeLocal, renderYouTubeUploadPreview } from './youtubeMetadata';

const clip = (timestamp = '2026-07-18T10:30:45Z'): RunClip => ({
	runId: 'dam-run',
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
		gameLanguage: 'en',
		romVersion: 'ntsc-u',
		sourceName: 'N64 Capture',
		comment: 'Created by test',
		pluginVersion: '1.2.3'
	},
	retentionState: 'kept',
	retentionReason: 'manual'
});

describe('YouTube datetime local helper', () => {
	const originalNavigator = globalThis.navigator;
	afterEach(() => {
		vi.unstubAllGlobals();
	});

	it('formats datetime_local with the browser locale', () => {
		const timestamp = '2026-07-18T10:30:45Z';

		vi.stubGlobal('navigator', { ...originalNavigator, languages: ['en-US'] });
		expect(formatDatetimeLocal(timestamp)).toBe(new Date(timestamp).toLocaleString('en-US'));
		expect(datetimeLocalForClip(clip(timestamp))).toBe(new Date(timestamp).toLocaleString('en-US'));

		vi.stubGlobal('navigator', { ...originalNavigator, languages: ['en-AU'] });
		expect(formatDatetimeLocal(timestamp)).toBe(new Date(timestamp).toLocaleString('en-AU'));
		expect(datetimeLocalForClip(clip(timestamp))).toBe(new Date(timestamp).toLocaleString('en-AU'));
	});

	it('falls back to the raw timestamp when the timestamp is invalid', () => {
		expect(formatDatetimeLocal('not a timestamp')).toBe('not a timestamp');
		expect(datetimeLocalForClip(clip('not a timestamp'))).toBe('not a timestamp');
	});
});

describe('YouTube upload preview rendering', () => {
	it('renders upload title, description, and visibility from the configured templates', () => {
		const preview = renderYouTubeUploadPreview(clip(), {
			titleTemplate: '{level} - {difficulty} - {time} - {rom}',
			descriptionTemplate:
				'{obs_replay_name}\n#{levelNumber}\n{status}\n{timestamp}\n{datetime_local}\n{plugin_version}',
			visibility: 'unlisted',
			datetimeLocal: 'July 18, 2026 at 10:30 AM'
		});

		expect(preview).toEqual({
			title: 'Dam - Agent - 01:23 - NTSC-U',
			description: 'dam\n#1\ncomplete\n2026-07-18T10:30:45Z\nJuly 18, 2026 at 10:30 AM\n1.2.3',
			visibility: 'unlisted',
			visibilityLabel: 'Unlisted'
		});
	});

	it('falls back to the clip stem when the rendered title is blank', () => {
		const preview = renderYouTubeUploadPreview(clip(), {
			titleTemplate: '   ',
			descriptionTemplate: '',
			visibility: 'private'
		});

		expect(preview.title).toBe('dam');
		expect(preview.visibilityLabel).toBe('Private');
	});

	it('renders an unknown ROM version explicitly', () => {
		const run = clip();
		run.metadata.romVersion = null;

		const preview = renderYouTubeUploadPreview(run, {
			titleTemplate: '{level} - {rom}',
			descriptionTemplate: 'ROM: {rom}',
			visibility: 'unlisted'
		});

		expect(preview.title).toBe('Dam - unknown');
		expect(preview.description).toBe('ROM: unknown');
	});
});
