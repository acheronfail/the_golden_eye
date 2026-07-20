import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import HomePage from './+page.svelte';
import { replayBuffer } from '$lib/replayBuffer.svelte';
import { obsSources } from '$lib/sources.svelte';
import { settings } from '$lib/settings.svelte';

const mocks = vi.hoisted(() => ({
	goto: vi.fn(),
	screenshotUrl: vi.fn((source: string) => `/api/v1/screenshot?source=${encodeURIComponent(source)}`)
}));

vi.mock('$app/navigation', () => ({
	goto: mocks.goto
}));

vi.mock('$lib/api', async (importOriginal) => {
	const actual = await importOriginal<typeof import('$lib/api')>();
	return {
		...actual,
		screenshotUrl: mocks.screenshotUrl
	};
});

beforeEach(() => {
	vi.clearAllMocks();
	obsSources.items = [{ name: 'N64 Capture', id: 'video_capture_device' }];
	obsSources.loaded = true;
	obsSources.version = 1;
	settings.showSourcePreviews = true;
	replayBuffer.status = {
		enabled: true,
		available: true,
		active: true,
		maxSeconds: 1200,
		outputDirectory: '/captures',
		defaultCompletedOutputPath: '/captures/GoldenEye',
		defaultFailedOutputPath: '/captures/GoldenEye/failed'
	};
	replayBuffer.loaded = true;
});

describe('home page', () => {
	it('selects a source and navigates to its monitor page', async () => {
		const user = userEvent.setup();
		render(HomePage);

		await user.click(screen.getByRole('button', { name: /N64 Capture/i }));

		expect(mocks.goto).toHaveBeenCalledWith('/sources/N64%20Capture');
	});

	it('hides source screenshots when previews are toggled off', async () => {
		const user = userEvent.setup();
		render(HomePage);

		expect(screen.getByAltText('Preview of N64 Capture')).toBeInTheDocument();

		await user.click(screen.getByRole('button', { name: /hide previews/i }));

		expect(settings.showSourcePreviews).toBe(false);
		expect(screen.queryByAltText('Preview of N64 Capture')).not.toBeInTheDocument();
		expect(screen.getByRole('button', { name: /show previews/i })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /N64 Capture/i })).toBeInTheDocument();
	});
});
