<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import RunImportDialog from '$lib/features/runs/RunImportDialog.svelte';

	const noop = () => {};
	const { Story } = defineMeta({
		title: 'Runs/Add times dialog',
		component: RunImportDialog,
		parameters: { layout: 'fullscreen' },
		args: {
			open: true,
			onClose: noop,
			onManual: noop,
			onElite: noop
		}
	});
</script>

<Story name="Manual entry" />
<Story name="The Elite import" args={{ initialMode: 'elite' }} />
<Story name="Importing" args={{ initialMode: 'elite', busy: 'elite' }} />
<Story
	name="Import complete"
	args={{ initialMode: 'elite', result: { imported: 84, alreadyImported: 0, videos: 73 } }}
/>
<Story name="Import failed" args={{ initialMode: 'elite', error: 'The Elite profile could not be found.' }} />
