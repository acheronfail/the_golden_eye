import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { backend, type YouTubeUploadStatus } from '$lib/api';
import { youtube } from '$lib/stores/youtube.svelte';
import { notifications } from '$lib/stores/notifications.svelte';
import { completedRun, connectedYouTube, uploadForRun } from '../../../stories/fixtures';
import AppHeader from '$lib/app/AppHeader.svelte';
import RunYouTubeSection from './RunYouTubeSection.svelte';

beforeEach(() => {
	youtube.applyStatus({ ...connectedYouTube, uploads: [] });
	youtube.cancellingUploadIds = [];
	youtube.uploadCancelErrors = {};
	youtube.error = null;
	notifications.flags = [];
});
afterEach(() => vi.restoreAllMocks());

function showUpload(upload: YouTubeUploadStatus) {
	youtube.applyUpload(upload);
	render(RunYouTubeSection, { clip: completedRun });
	return render(AppHeader, {
		links: [],
		currentPath: '/runs',
		pluginVersion: 'test',
		youtubeConnected: true,
		uploadsOpen: true,
		uploads: [upload]
	});
}

describe('YouTube upload run identity', () => {
	it('shows and updates an automatic upload when its saved path differs from the catalog path', async () => {
		const upload = uploadForRun('uploading', { path: '/saved-runs/personal-best.mp4', progressRatio: 0.25 });
		youtube.handleUploadChanged(upload);
		youtube.handlePersonalBestUploadStarted(upload);
		render(RunYouTubeSection, { clip: completedRun });

		expect(notifications.flags[0].href).toBe(`/runs?runId=${completedRun.runId}`);
		expect(screen.getByRole('button', { name: 'Uploading 25%...' })).toBeDisabled();
		expect(screen.getByRole('progressbar', { name: 'YouTube upload progress' })).toHaveAttribute('aria-valuenow', '25');
		expect(screen.getByRole('button', { name: `Cancel upload: ${upload.title}` })).toBeEnabled();

		youtube.handleUploadChanged({ ...upload, progressRatio: 0.75 });
		await waitFor(() =>
			expect(screen.getByRole('progressbar', { name: 'YouTube upload progress' })).toHaveAttribute(
				'aria-valuenow',
				'75'
			)
		);
		youtube.handleUploadChanged({ ...upload, state: 'processing' });
		await waitFor(() => expect(screen.getByRole('button', { name: 'Processing...' })).toBeDisabled());
		youtube.handleUploadChanged({ ...upload, state: 'uploaded', videoUrl: 'https://youtu.be/pb-video' });
		await waitFor(() => expect(screen.getByText('Uploaded to YouTube.')).toBeInTheDocument());
	});

	it('does not show an upload for a different run that reused the same path', () => {
		youtube.applyUpload(uploadForRun('uploading', { runId: 'previous-run' }));
		render(RunYouTubeSection, { clip: completedRun });
		expect(screen.getByRole('button', { name: 'Upload' })).toBeEnabled();
		expect(screen.queryByRole('progressbar')).not.toBeInTheDocument();
	});
});

describe('YouTube upload cancellation', () => {
	it.each(['queued', 'uploading'] as const)(
		'cancels a %s upload from either progress display and permits retry',
		async (state) => {
			const upload = uploadForRun(state);
			let finish!: (status: YouTubeUploadStatus) => void;
			const cancel = vi.spyOn(backend, 'cancelYouTubeUpload').mockImplementation(
				() =>
					new Promise((resolve) => {
						finish = resolve;
					})
			);
			const header = showUpload(upload);
			const buttons = screen.getAllByRole('button', { name: `Cancel upload: ${upload.title}` });
			expect(buttons).toHaveLength(2);
			await fireEvent.click(buttons[state === 'queued' ? 0 : 1]);
			expect(cancel).toHaveBeenCalledExactlyOnceWith(upload.id);
			await header.rerender({ uploadsOpen: true });
			await waitFor(() => {
				for (const button of screen.getAllByRole('button', { name: `Cancel upload: ${upload.title}` }))
					expect(button).toBeDisabled();
			});
			const cancelled = { ...upload, state: 'cancelled' as const };
			youtube.handleUploadChanged(cancelled);
			finish({ ...upload, state: 'cancelling' });
			await waitFor(() => expect(youtube.cancellingUploadIds).toHaveLength(0));
			expect(youtube.uploadForRun(upload.runId)?.state).toBe('cancelled');
			await header.rerender({ uploads: youtube.uploads });
			expect(screen.getByText('No current uploads.')).toBeInTheDocument();
			expect(screen.getByText('Upload cancelled. You can upload this clip again.')).toBeInTheDocument();
			expect(screen.getByRole('button', { name: 'Upload' })).toBeEnabled();
			expect(notifications.flags).toHaveLength(0);
		}
	);

	it('shows cancellation errors in both displays and allows another attempt', async () => {
		const upload = uploadForRun('uploading');
		vi.spyOn(backend, 'cancelYouTubeUpload').mockRejectedValue(new Error('Connection lost'));
		showUpload(upload);
		await fireEvent.click(screen.getAllByRole('button', { name: `Cancel upload: ${upload.title}` })[1]);
		await waitFor(() => expect(screen.getAllByRole('alert')).toHaveLength(2));
		for (const alert of screen.getAllByRole('alert')) expect(alert).toHaveTextContent('Connection lost');
		for (const button of screen.getAllByRole('button', { name: `Cancel upload: ${upload.title}` }))
			expect(button).toBeEnabled();
		expect(youtube.uploadForRun(upload.runId)?.state).toBe('uploading');
	});

	it('keeps completion and its video link when cancellation loses the race', async () => {
		const upload = uploadForRun('cancelling', { id: 'completion-race' });
		showUpload(upload);
		youtube.handleUploadChanged({
			...upload,
			state: 'uploaded',
			videoId: 'video-123',
			videoUrl: 'https://youtu.be/video-123'
		});
		youtube.applyUpload({ ...upload, state: 'uploading' });
		await waitFor(() => expect(screen.getByText('Uploaded to YouTube.')).toBeInTheDocument());
		expect(youtube.uploadForRun(upload.runId)?.videoId).toBe('video-123');
		expect(notifications.flags.map((flag) => flag.title)).toContain('Upload completed before cancellation');
	});
});
