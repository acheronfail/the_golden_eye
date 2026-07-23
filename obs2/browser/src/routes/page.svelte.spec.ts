import { render, screen } from '@testing-library/svelte';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import HomePage from './+page.svelte';
import { replayBuffer } from '$lib/stores/replayBuffer.svelte';
import { obsSources } from '$lib/stores/sources.svelte';
import { settings } from '$lib/stores/settings.svelte';

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
	settings.lastUsedSourceName = null;
	replayBuffer.status = {
		enabled: true,
		available: true,
		active: true,
		maxSeconds: 1200,
		outputDirectory: '/captures',
		defaultCompletedOutputPath: '/captures/GoldenEye'
	};
	replayBuffer.loaded = true;
});

describe('home page', () => {
	it('selects a source and navigates to its monitor page', async () => {
		const user = userEvent.setup();
		render(HomePage);

		await user.click(screen.getByRole('button', { name: /N64 Capture/i }));

		expect(mocks.goto).toHaveBeenCalledWith('/sources/N64%20Capture');
		expect(settings.lastUsedSourceName).toBe('N64 Capture');
	});

	it('pins an available last used source above the remaining sources', () => {
		obsSources.items = [
			{ name: 'Capture Card', id: 'decklink_input' },
			{ name: 'N64 Capture', id: 'video_capture_device' }
		];
		settings.lastUsedSourceName = 'N64 Capture';

		render(HomePage);

		const choices = screen
			.getAllByRole('button')
			.filter((button) => ['N64 Capture', 'Capture Card'].some((name) => button.textContent?.includes(name)));
		expect(choices.map((button) => button.textContent)).toEqual([
			expect.stringContaining('N64 Capture'),
			expect.stringContaining('Capture Card')
		]);
		expect(choices[0]).toHaveTextContent('video_capture_device');
		expect(screen.getByRole('heading', { name: 'last used source' })).toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'sources' })).toBeInTheDocument();
	});

	it('falls back to the normal list when the last used source is unavailable', () => {
		settings.lastUsedSourceName = 'Disconnected Capture';

		render(HomePage);

		expect(screen.queryByRole('heading', { name: 'last used source' })).not.toBeInTheDocument();
		expect(screen.getByRole('heading', { name: 'sources' })).toBeInTheDocument();
		expect(screen.getByRole('button', { name: /N64 Capture/i })).toBeInTheDocument();
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
