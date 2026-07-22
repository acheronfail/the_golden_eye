import { describe, expect, it } from 'vitest';
import { youtubePathKeyForPlatform, youtubePathsMatchForPlatform } from '$lib/stores/youtube.svelte';

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
