<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import RunListStory from './RunListStory.svelte';
	import { runClips } from '../../fixtures';
	import { readmeRuns } from '../../readmeFixtures';

	const { Story } = defineMeta({
		title: 'Runs/Run list',
		component: RunListStory,
		parameters: { layout: 'fullscreen' }
	});
</script>

<Story name="Loading folders" args={{ loading: true, clips: [], visibleClips: [], scannedDirectoryCount: 0 }} />
<Story name="Probing clips" args={{ loading: true, clips: [], visibleClips: [], scannedDirectoryCount: 2 }} />
<Story name="No folders configured" args={{ clips: [], visibleClips: [], directoryCount: 0 }} />
<Story name="No tagged clips" args={{ clips: [], visibleClips: [], directoryCount: 2 }} />
<Story name="Different runs" args={{ clips: runClips, visibleClips: runClips }} />
<Story
	name="Run actions above grouped rows"
	args={{ clips: runClips }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);
		await userEvent.click(canvas.getAllByRole('button', { name: 'More actions' })[0]);
		const menu = within(canvas.getByRole('menu'));
		for (const item of menu.getAllByRole('menuitem')) {
			const { x, y, width, height } = item.getBoundingClientRect();
			const hit = item.ownerDocument.elementFromPoint(x + width / 2, y + height / 2);
			await expect(item.contains(hit)).toBe(true);
		}
		await userEvent.click(menu.getByRole('menuitem', { name: 'Delete' }));
		await expect(canvas.queryByRole('menu')).toBeNull();
	}}
/>
<Story name="README preview" args={{ clips: readmeRuns }} />
<Story name="Virtualized 116-run history" args={{ clips: runClips, generatedRunCount: 116 }} />
<Story name="Virtualized 5,000-run same-date history" args={{ clips: runClips, generatedRunCount: 5000 }} />
<Story
	name="Infinite 10,000-run history first page"
	args={{ clips: runClips, generatedRunCount: 50, total: 10000, hasMore: true }}
/>
<Story name="Scanning with results" args={{ loading: true, clips: runClips, visibleClips: runClips }} />
<Story name="No filter matches" args={{ clips: [], visibleClips: [], total: 0, hasActiveFilters: true }} />
