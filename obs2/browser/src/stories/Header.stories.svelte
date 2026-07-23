<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import AppHeader, { type AppHeaderLink } from '$lib/components/AppHeader.svelte';

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
