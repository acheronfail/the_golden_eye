<script lang="ts">
	import { apiUrl, wsUrl } from '$lib/api';
	import { settings } from '$lib/settings.svelte';

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
	let monitoring = $state<string | null>(null);
	let matchSocket: WebSocket | null = null;
	let match = $state<LevelMatch | null>(null);

	const getSources = async () => {
		const res = await fetch(apiUrl('/api/v1/sources'));
		const data = await res.json();
		sources = data;
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
</script>

<div>
	<h1 class="mb-4 text-2xl font-bold">Welcome to Goldeneye!</h1>
	<p class="mb-4">This is the main dashboard.</p>

	<fieldset class="mb-4">
		<legend class="mb-2 font-semibold">Language:</legend>
		<div class="flex flex-col pl-4">
			<label class="mr-4">
				<input type="radio" name="lang" value="en" bind:group={settings.lang} />
				English
			</label>
			<label>
				<input type="radio" name="lang" value="jp" bind:group={settings.lang} />
				Japanese
			</label>
		</div>
	</fieldset>

	<button
		class="mb-4 rounded bg-blue-500 px-4 py-2 font-semibold text-white hover:bg-blue-600"
		onclick={getSources}>get sources</button
	>

	{#if sources.length == 0}
		<p class="mb-4 text-gray-500">No sources, click "get sources" to fetch them from OBS.</p>
	{:else}
		<div class="flex flex-col gap-4">
			<h2 class="mb-2 text-xl font-semibold">Available Sources:</h2>

			<ul class="grid grid-cols-[max-content_1fr] items-center gap-x-4 gap-y-3">
				{#each sources as source}
					<li class="contents">
						<span class="text-right font-mono">{source.name}: </span>

						<div class="flex flex-wrap gap-2">
							{#if knownVideoSourceIds.includes(source.id)}
								{#if !monitoring}
									<button
										class="rounded bg-green-500 px-2 py-1 text-white hover:bg-green-600"
										onclick={startMonitor(source.name)}>start monitor</button
									>
								{:else if monitoring === source.name}
									<button
										class="rounded bg-red-500 px-2 py-1 text-white hover:bg-red-600"
										onclick={stopMonitor}>stop monitor</button
									>
								{/if}
							{:else}
								<span class="font-mono text-gray-400">(not a video source)</span>
							{/if}
						</div>
					</li>
				{/each}
			</ul>
		</div>
	{/if}

	{#if match}
		<pre>{JSON.stringify(match, null, 2)}</pre>
	{/if}
</div>
