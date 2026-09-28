import { fireEvent, render, screen } from '@testing-library/svelte';
import type { YouTubeUploadStatus } from '$lib/api';
import { describe, expect, it } from 'vitest';
import AppHeader, { type AppHeaderLink } from './AppHeader.svelte';

const links: AppHeaderLink[] = [
	{ href: '/', label: 'Monitor' },
	{ href: '/runs', label: 'Runs' },
	{ href: '/options', label: 'Options' }
];

describe('AppHeader', () => {
	it('only highlights the current page when monitoring is active', () => {
		render(AppHeader, {
			links,
			currentPath: '/options',
			pluginVersion: 'test',
			activeMonitorHref: '/sources/Nintendo%2064',
			menuOpen: true
		});

		const monitor = screen.getByRole('link', { name: 'Monitor' });
		const options = screen.getByRole('link', { name: 'Options' });

		expect(monitor).not.toHaveClass('obs-phase-waiting-button');
		expect(monitor).not.toHaveClass('obs-menu-link-active');
		expect(monitor).not.toHaveAttribute('aria-current');
		expect(options).toHaveClass('obs-menu-link-active');
		expect(options).toHaveClass('border');
		expect(options).toHaveAttribute('aria-current', 'page');
	});

	it('uses the normal active-menu style on a live monitor route', () => {
		render(AppHeader, {
			links,
			currentPath: '/sources/Nintendo%2064',
			pluginVersion: 'test',
			activeMonitorHref: '/sources/Nintendo%2064',
			menuOpen: true
		});

		const monitor = screen.getByRole('link', { name: 'Monitor' });
		expect(monitor).toHaveClass('obs-menu-link-active');
		expect(monitor).not.toHaveClass('obs-phase-waiting-button');
		expect(monitor).toHaveAttribute('aria-current', 'page');
	});
});

const upload = (state: YouTubeUploadStatus['state'], id = state): YouTubeUploadStatus => ({
	id,
	runId: 'run/with spaces',
	path: '/clips/dam.mp4',
	fileName: 'dam.mp4',
	title: `Dam ${id}`,
	state,
	progressRatio: 0.42,
	progressBytes: 420,
	totalBytes: 1000,
	videoId: null,
	videoUrl: null,
	error: null,
	startedAt: '2026-09-28T00:00:00Z',
	finishedAt: null
});
const props = { links, currentPath: '/', pluginVersion: 'test', activeMonitorHref: '/sources/N64' };

describe('header uploads', () => {
	it('follows authentication and only compacts monitoring while connected', async () => {
		const { rerender } = render(AppHeader, props);
		expect(screen.queryByRole('button', { name: /Uploads/ })).not.toBeInTheDocument();
		expect(screen.getByText('Monitoring')).toBeInTheDocument();
		await rerender({ ...props, youtubeConnected: true });
		expect(screen.getByRole('button', { name: 'Uploads' })).toHaveTextContent('');
		expect(screen.queryByText('Monitoring')).not.toBeInTheDocument();
		expect(screen.getByRole('link', { name: 'Return to monitoring screen' })).toBeInTheDocument();
		await fireEvent.click(screen.getByRole('button', { name: 'Uploads' }));
		expect(screen.getByText('No current uploads.')).toBeInTheDocument();
		await rerender({ ...props, youtubeConnected: false });
		expect(screen.queryByRole('region', { name: 'Current uploads' })).not.toBeInTheDocument();
		expect(screen.getByText('Monitoring')).toBeInTheDocument();
	});

	it('updates progress and the badge, excluding completed and failed uploads', async () => {
		const { rerender } = render(AppHeader, {
			...props,
			youtubeConnected: true,
			uploadsOpen: true,
			uploads: [upload('uploading'), upload('queued'), upload('uploaded'), upload('failed')]
		});
		expect(screen.getByRole('button', { name: 'Uploads (2 active)' })).toHaveTextContent('2');
		expect(screen.getAllByRole('progressbar')).toHaveLength(2);
		expect(screen.getByRole('progressbar', { name: 'Dam uploading' })).toHaveAttribute('aria-valuenow', '42');
		expect(screen.getByRole('progressbar', { name: 'Dam queued' })).toHaveAttribute('aria-valuenow', '0');
		expect(screen.getByRole('link', { name: 'Dam uploading' })).toHaveAttribute(
			'href',
			'/runs?runId=run%2Fwith%20spaces'
		);
		await rerender({ uploads: [{ ...upload('uploading'), progressRatio: 0.81 }] });
		expect(screen.getByRole('progressbar')).toHaveAttribute('aria-valuenow', '81');
		expect(screen.getByRole('button', { name: 'Uploads (1 active)' })).toHaveTextContent('1');
		await rerender({ uploads: [upload('uploaded'), upload('failed')] });
		expect(screen.getByText('No current uploads.')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Uploads' })).toHaveTextContent('');
	});

	it('dismisses on Escape and outside clicks and keeps popovers mutually exclusive', async () => {
		render(AppHeader, { ...props, youtubeConnected: true });
		const button = screen.getByRole('button', { name: 'Uploads' });
		await fireEvent.click(button);
		await fireEvent.keyDown(window, { key: 'Escape' });
		expect(button).toHaveFocus();
		expect(button).toHaveAttribute('aria-expanded', 'false');
		await fireEvent.click(button);
		await fireEvent.click(document.body);
		expect(button).toHaveAttribute('aria-expanded', 'false');
		await fireEvent.click(screen.getByRole('button', { name: 'Open navigation menu' }));
		await fireEvent.click(button);
		expect(screen.queryByRole('navigation')).not.toBeInTheDocument();
		await fireEvent.click(screen.getByRole('button', { name: 'Open navigation menu' }));
		expect(screen.queryByRole('region', { name: 'Current uploads' })).not.toBeInTheDocument();
		expect(screen.getByRole('navigation')).toBeInTheDocument();
	});
});
