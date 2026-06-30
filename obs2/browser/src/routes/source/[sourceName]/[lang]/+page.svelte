<script lang="ts">
	import { afterNavigate, goto } from '$app/navigation';
	import {
		connectMonitorSocket,
		getMonitorStatus,
		getSources,
		startMonitor as apiStartMonitor,
		stopMonitor as apiStopMonitor,
		type LevelMatch
	} from '$lib/api';
	import WizardFrame from '$lib/wizard/WizardFrame.svelte';
	import OptionList, { type Option } from '$lib/wizard/OptionList.svelte';
	import type { PageProps } from './$types';

	let { params }: PageProps = $props();

	let monitoring = $state(false);
	let matchSocket: WebSocket | null = null;
	let match = $state<LevelMatch | null>(null);

	// The source name comes from the URL, so it may be stale if the user navigated
	// here from browser history / a manual URL, or renamed the source in OBS. Verify
	// it still exists before letting them monitor; the start button stays disabled
	// until this completes, and a failed check routes back home to restart the flow.
	let verified = $state(false);

	// Runs on initial load and after every navigation. A redirect below targets
	// this same route, so the component instance is reused and `onMount` wouldn't
	// fire again -- `afterNavigate` re-runs the restore each time the params change.
	afterNavigate(async () => {
		verified = false;
		try {
			// If a monitor is already running, restore that state rather than
			// defaulting to idle. When it's running for a different source/lang than
			// this URL describes, redirect to the page that matches reality.
			const status = await getMonitorStatus();
			if (status.enabled) {
				if (status.sourceName !== params.sourceName || status.lang !== params.lang) {
					goto(`/source/${encodeURIComponent(status.sourceName)}/${status.lang}`);
					return;
				}
				monitoring = true;
				connectMatchSocket();
			} else {
				monitoring = false;
				match = null;
				disconnectMatchSocket();
			}

			const sources = await getSources();
			if (sources.some((source) => source.name === params.sourceName)) {
				verified = true;
			} else {
				goto('/');
			}
		} catch {
			goto('/');
		}
	});

	const connectMatchSocket = () => {
		matchSocket?.close();
		const socket = connectMonitorSocket(
			(m) => {
				match = m;
				console.log('level match', match);
			},
			() => {
				if (matchSocket === socket) matchSocket = null;
			}
		);
		matchSocket = socket;
	};
	const disconnectMatchSocket = () => {
		matchSocket?.close();
		matchSocket = null;
	};

	// Guards against a double start: the window Space handler and the OptionList
	// button's own keyboard activation can both fire for one keypress.
	let starting = false;
	const startMonitor = async () => {
		if (monitoring || starting) return;
		starting = true;
		try {
			await apiStartMonitor(params.sourceName, params.lang);
			monitoring = true;
			connectMatchSocket();
		} catch (err) {
			alert(err instanceof Error ? err.message : String(err));
		} finally {
			starting = false;
		}
	};

	const stopMonitor = async () => {
		try {
			await apiStopMonitor();
			monitoring = false;
			match = null;
			disconnectMatchSocket();
		} catch (err) {
			alert(err instanceof Error ? err.message : String(err));
		}
	};

	const option: Option = {
		title: 'start monitor',
		detail: 'or press space to start monitoring'
	};

	// Space toggles monitoring from anywhere on the page (without relying on the
	// OptionList button holding focus); Escape also stops it. `startMonitor` is
	// idempotent, so it's harmless if the button's native activation also fires.
	const onkeydown = (event: KeyboardEvent) => {
		if (monitoring) {
			if (event.key === ' ' || event.key === 'Escape') {
				event.preventDefault();
				stopMonitor();
			}
		} else if (event.key === ' ' && verified) {
			event.preventDefault();
			startMonitor();
		}
	};
</script>

<svelte:head>
	<title>Monitor | {params.lang} | {params.sourceName}</title>
</svelte:head>

<svelte:window {onkeydown} />

{#if monitoring}
	<main class="relative flex flex-1 flex-col items-center justify-center px-6 py-12 text-center">
		<!-- A thick border frames the page area (below the header) to signal monitoring is live. -->
		<div class="pointer-events-none absolute inset-0 z-10 border-8 border-amber-500"></div>

		<p class="font-mono text-xs tracking-widest text-amber-500 uppercase">
			Monitoring
			<span>({params.lang})</span>
		</p>
		<h1 class="mt-4 text-6xl font-semibold wrap-break-word text-amber-300">
			{match?.screen ?? '…'}
		</h1>
		<p class="mt-6 text-sm text-neutral-400">press escape or space to stop monitoring</p>
	</main>
{:else}
	<WizardFrame
		step={3}
		title="Ready to monitor"
		subtitle="{params.lang} | {params.sourceName}"
		hrefs={['/', `/source/${encodeURIComponent(params.sourceName)}`]}
	>
		{#if verified}
			<OptionList options={[option]} onSelect={startMonitor} />
		{:else}
			<p class="font-mono text-sm text-neutral-500">Verifying source…</p>
		{/if}
	</WizardFrame>
{/if}
