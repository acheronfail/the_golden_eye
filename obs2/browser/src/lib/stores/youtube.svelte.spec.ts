import { beforeEach, describe, expect, it } from 'vitest';
import { youtube, youtubePathKeyForPlatform, youtubePathsMatchForPlatform } from '$lib/stores/youtube.svelte';
import { completedRun, connectedYouTube, uploadForRun } from '../../stories/fixtures';

describe('YouTube upload selection', () => {
	beforeEach(() => youtube.applyStatus({ ...connectedYouTube, uploads: [] }));

	it('prefers an active attempt for the run over a later terminal attempt', () => {
		youtube.applyUpload(uploadForRun('uploading', { path: '/saved-runs/pb.mp4' }));
		youtube.applyUpload(uploadForRun('failed', { startedAt: '2026-07-21T12:48:00Z' }));
		expect(youtube.uploadForRun(completedRun.runId)?.state).toBe('uploading');
	});

	it('returns the latest terminal attempt for the run', () => {
		youtube.applyUpload(uploadForRun('failed'));
		youtube.applyUpload(uploadForRun('uploaded', { startedAt: '2026-07-21T12:48:00Z' }));
		expect(youtube.uploadForRun(completedRun.runId)?.state).toBe('uploaded');
		expect(youtube.uploadForRun('another-run')).toBeNull();
	});
});

describe('YouTube upload path matching', () => {
	it('matches case-insensitively on macOS', () => {
		expect(
			youtubePathsMatchForPlatform(
				'/Users/example/Movies/GoldenEye/Runway/run.mov',
				'/Users/example/Movies/Goldeneye/Runway/run.mov',
				'MacIntel'
			)
		).toBe(true);
	});

	it('matches case-insensitively on Windows and normalizes separators', () => {
		expect(
			youtubePathsMatchForPlatform(
				'C:\\Users\\Example\\Movies\\GoldenEye\\run.mov',
				'C:/Users/Example/Movies/goldeneye/run.mov',
				'Win32'
			)
		).toBe(true);
	});

	it('preserves case sensitivity on Linux', () => {
		expect(
			youtubePathsMatchForPlatform(
				'/home/example/Movies/GoldenEye/run.mov',
				'/home/example/Movies/Goldeneye/run.mov',
				'Linux x86_64'
			)
		).toBe(false);
	});

	it('uses a stable normalized key for matching', () => {
		expect(youtubePathKeyForPlatform('C:\\Runs\\GoldenEye\\clip.mov', 'Win32')).toBe('c:/runs/goldeneye/clip.mov');
	});
});
