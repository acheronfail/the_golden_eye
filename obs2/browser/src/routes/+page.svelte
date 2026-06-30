<script lang="ts">
	import { apiUrl, wsUrl } from '$lib/api';
	import { settings } from '$lib/settings.svelte';
	import { onMount } from 'svelte';
	import InputLang from '../lib/InputLang.svelte';

	const knownVideoSourceIds = [
		'screen_capture',
		'macos-avcapture',
		'macos-avcapture-fast',
		'ffmpeg_source',
		'v4l2_input'
	];

	/** The level match the backend pushes over the monitor WebSocket. Mirrors
	 * the Rust `LevelMatch` struct (`runtime_ms` is included but the backend
	 * only pushes a new message when the rest of the state changes). */
	type LevelMatch = {
		screen: string;
		mission: number;
		part: number;
		difficulty: number;
		times: number[];
		runtime_ms: number;
	};

	let sources = $state<{ name: string; id: string }[]>([]);
	let sourcesLoading = $state(false);
	let monitoring = $state<string | null>(null);
	let matchSocket: WebSocket | null = null;
	let match = $state<LevelMatch | null>(null);

	const getSources = async () => {
		sourcesLoading = true;
		const res = await fetch(apiUrl('/api/v1/sources'));
		const data = await res.json();
		sources = data;
		setTimeout(() => (sourcesLoading = false), 250);
	};

	// Open a WebSocket to the backend that pushes the latest LevelMatch (as JSON)
	// whenever the matched state changes. For now we just log it; the UI will be
	// built later.
	const connectMatchSocket = () => {
		matchSocket?.close();
		const socket = new WebSocket(wsUrl('/api/v1/monitor/ws'));
		socket.onmessage = (event) => {
			match = JSON.parse(event.data) as LevelMatch;
			console.log('level match', match);
		};
		socket.onclose = () => {
			if (matchSocket === socket) matchSocket = null;
		};
		matchSocket = socket;
	};
	const disconnectMatchSocket = () => {
		matchSocket?.close();
		matchSocket = null;
	};

	const startMonitor = (sourceName: string) => async () => {
		const res = await fetch(apiUrl(`/api/v1/monitor/start`), {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ sourceName, lang: settings.lang })
		});
		if (res.ok) {
			monitoring = sourceName;
			connectMatchSocket();
		} else {
			alert(`Request error: ${res.status} ${await res.text()}`);
		}
	};

	const stopMonitor = async () => {
		const res = await fetch(apiUrl(`/api/v1/monitor/stop`), {
			method: 'POST',
			headers: { 'content-type': 'application/json' }
		});
		if (res.ok) {
			monitoring = null;
			match = null;
			disconnectMatchSocket();
		} else {
			alert(`Request error: ${res.status} ${await res.text()}`);
		}
	};

	onMount(() => {
		// FIXME: this doesn't stop the monitor when a refresh happens, but we should do that
		return async () => {
			if (monitoring) {
				await stopMonitor();
			}
		};
	});
</script>

<div class="flex flex-col gap-4 p-4">
	<h1 class="text-2xl font-bold">Welcome to Goldeneye!</h1>
	<p>Make sure to select the right language for the version of Goldeneye you're using!</p>

	<InputLang />

	<div class="flex flex-col gap-4">
		<div class="flex flex-row gap-2">
			<h2 class="text-xl font-semibold">Available Sources:</h2>
			<button
				class="rounded bg-blue-500 px-2 py-1 font-semibold text-white hover:bg-blue-600 disabled:bg-slate-500 disabled:text-slate-300"
				disabled={sourcesLoading}
				onclick={getSources}>refresh sources</button
			>
		</div>

		{#if sources.length == 0}
			<p class="text-gray-500">No sources, click "refresh sources" to fetch them from OBS.</p>
		{:else}
			<ul class="grid grid-cols-[max-content_1fr] items-center gap-x-4 gap-y-3">
				{#each sources as source}
					<li class="contents">
						<span class="text-right font-mono">{source.name}: </span>

						<div class="flex flex-wrap gap-2">
							{#if knownVideoSourceIds.includes(source.id)}
								{#if monitoring === source.name}
									<button
										class="rounded bg-red-500 px-2 py-1 text-white hover:bg-red-600"
										onclick={stopMonitor}>stop monitor</button
									>
								{:else}
									<button
										class="rounded bg-green-500 px-2 py-1 text-white hover:bg-green-600 disabled:bg-slate-500 disabled:text-slate-300"
										disabled={!!monitoring}
										onclick={startMonitor(source.name)}>start monitor</button
									>
								{/if}
							{:else}
								<span class="font-mono text-gray-400">(not a video source)</span>
							{/if}
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	{#if match}
		<div class="grid grid-cols-[max-content_1fr] items-center gap">
			<span>Screen:</span>
			<span>{match.screen}</span>

			{#if match.difficulty !== -1}
				<span>Difficulty:</span>
				<span>{match.difficulty}</span>
			{/if}

			{#if match.mission !== -1 && match.part !== -1}
				<span>mission:</span>
				<span>{match.mission}</span>
				<span>part: {match.part}</span>
				<span>{match.part}</span>
			{/if}

			{#if match.times}
				<span>Times:</span>
				<pre>{JSON.stringify(match.times)}</pre>
			{/if}
		</div>
	{/if}
</div>
