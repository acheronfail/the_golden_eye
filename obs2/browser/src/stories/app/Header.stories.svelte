<script module lang="ts">
	import type { YouTubeUploadStatus } from '$lib/api';
	import { expect, userEvent, within } from 'storybook/test';
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import AppHeader, { type AppHeaderLink } from '$lib/app/AppHeader.svelte';

	const upload = (
		id: string,
		title: string,
		state: YouTubeUploadStatus['state'],
		progressRatio: number | null
	): YouTubeUploadStatus => ({
		id,
		runId: `run-${id}`,
		path: `/clips/${id}.mp4`,
		fileName: `${id}.mp4`,
		title,
		state,
		progressRatio,
		progressBytes: (progressRatio ?? 0) * 1000,
		totalBytes: 1000,
		videoId: null,
		videoUrl: null,
		error: null,
		startedAt: '2026-09-28T00:00:00Z',
		finishedAt: null
	});
	const uploads = [
		upload('1', 'Dam — Agent — 0:52', 'uploading', 0.42),
		upload('2', 'Facility — Secret Agent — 0:53', 'uploading', 0.76),
		upload('3', 'Archives — 00 Agent — 0:54', 'queued', 0)
	];

	const links: AppHeaderLink[] = [
		{ href: '/', label: 'Monitor' },
		{ href: '/runs', label: 'Runs' },
		{ href: '/options', label: 'Options' }
	];

	const { Story } = defineMeta({
		title: 'App/Header',
		component: AppHeader,
		parameters: { layout: 'fullscreen' },
		args: {
			links,
			currentPath: '/',
			pluginVersion: '2.4.0',
			activeMonitorHref: null,
			recordingState: null,
			monitorPhase: null,
			menuOpen: false
		}
	});
</script>

<Story name="Default" />
<Story name="Navigation open" args={{ menuOpen: true }} />
<Story name="Runs active" args={{ currentPath: '/runs', menuOpen: true }} />
<Story
	name="Developer navigation"
	args={{ links: [...links, { href: '/developer', label: 'Developer' }], currentPath: '/developer', menuOpen: true }}
/>
<Story name="Monitoring waiting" args={{ activeMonitorHref: '/sources/Nintendo%2064' }} />
<Story
	name="Navigation while monitoring"
	args={{ currentPath: '/options', activeMonitorHref: '/sources/Nintendo%2064', menuOpen: true }}
/>
<Story name="Monitor verifying" args={{ monitorPhase: 'neutral' }} />
<Story name="Monitoring recording" args={{ activeMonitorHref: '/sources/Nintendo%2064', recordingState: 'started' }} />
<Story name="Monitoring failed" args={{ activeMonitorHref: '/sources/Nintendo%2064', recordingState: 'failed' }} />
<Story name="Monitoring complete" args={{ activeMonitorHref: '/sources/Nintendo%2064', recordingState: 'complete' }} />

<Story
	name="Uploads connected idle"
	args={{ youtubeConnected: true }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		const button = canvas.getByRole('button', { name: 'Uploads' });
		await userEvent.click(button);
		await expect(canvas.getByText('No uploads this session.')).toBeVisible();
		await userEvent.keyboard('{Escape}');
		await expect(button).toHaveFocus();
	}}
/>
<Story name="Uploads empty" args={{ youtubeConnected: true, uploadsOpen: true }} />
<Story name="Uploads active badge" args={{ youtubeConnected: true, uploads }} />
<Story
	name="Uploads active and queued"
	args={{ youtubeConnected: true, uploadsOpen: true, uploads }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getByRole('button', { name: 'Uploads (3 active)' })).toBeVisible();
		await expect(canvas.getAllByRole('progressbar')).toHaveLength(3);
		canvas.getByRole('button', { name: 'Uploads (3 active)' }).focus();
		await userEvent.tab();
		await expect(canvas.getByRole('link', { name: uploads[0].title })).toHaveFocus();
		await expect(canvas.getByRole('link', { name: uploads[0].title })).toHaveAttribute('href', '/runs?runId=run-1');
	}}
/>
<Story
	name="Uploads while monitoring"
	args={{
		youtubeConnected: true,
		uploadsOpen: true,
		uploads,
		activeMonitorHref: '/sources/Nintendo%2064',
		recordingState: 'started'
	}}
/>
<Story name="Uploads queued only" args={{ youtubeConnected: true, uploadsOpen: true, uploads: [uploads[2]] }} />
<Story
	name="Uploads unknown progress"
	args={{ youtubeConnected: true, uploadsOpen: true, uploads: [upload('4', 'Silo — Agent', 'uploading', null)] }}
/>
<Story
	name="Uploads long queue"
	args={{
		youtubeConnected: true,
		uploadsOpen: true,
		activeMonitorHref: '/sources/Nintendo%2064',
		uploads: [
			...uploads,
			...Array.from({ length: 10 }, (_, i) =>
				upload(
					String(i + 4),
					'A very long upload title with details about this GoldenEye run — Train — 00 Agent',
					'queued',
					0
				)
			)
		]
	}}
/>
<Story
	name="Uploads finished"
	args={{
		youtubeConnected: true,
		uploadsOpen: true,
		uploads: [
			{ ...uploads[0], state: 'uploaded' },
			{ ...uploads[1], state: 'failed' },
			{ ...uploads[2], state: 'cancelled' }
		]
	}}
/>

<Story
	name="Cancelling upload"
	args={{ youtubeConnected: true, uploadsOpen: true, uploads: [{ ...uploads[0], state: 'cancelling' }] }}
/>
<Story
	name="Cancelled upload"
	args={{ youtubeConnected: true, uploadsOpen: true, uploads: [{ ...uploads[0], state: 'cancelled' }] }}
/>

<Story
	name="Uploads with session history"
	args={{
		youtubeConnected: true,
		uploadsOpen: true,
		uploads: [
			...uploads,
			...Array.from({ length: 12 }, (_, i) => ({
				...upload(`history-${i}`, `Completed run ${i + 1}`, 'uploaded', 1),
				finishedAt: `2026-09-28T00:00:${String(i).padStart(2, '0')}Z`
			}))
		]
	}}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await expect(canvas.getAllByRole('listitem')).toHaveLength(13);
		await expect(canvas.getAllByRole('progressbar')).toHaveLength(3);
		await expect(canvas.getByRole('button', { name: 'Uploads (3 active)' })).toBeVisible();
		await expect(canvas.queryByRole('link', { name: 'Completed run 1' })).not.toBeInTheDocument();
	}}
/>
