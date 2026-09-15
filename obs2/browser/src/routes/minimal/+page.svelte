<script lang="ts">
	import { onMount } from 'svelte';
	import { backend, type AppEvent, type LevelMatch, type RecordingStatus } from '$lib/api';
	import MinimalOverlay from '$lib/features/minimal/MinimalOverlay.svelte';

	const storageKey = 'the-golden-eye.minimal.font-size';
	const timesStorageKey = 'the-golden-eye.minimal.show-times';
	const clampSize = (value: number) => Math.max(6, Math.min(128, value));
	let fontSize = $state(16);
	let showTimes = $state(true);
	let connected = $state(false);
	let enabled = $state(false);
	let times = $state<LevelMatch['times']>(null);
	let recordingState = $state<RecordingStatus | null>(null);

	let sourceName: string | null = null;
	let transition = $state<'starting' | 'stopping' | null>(null);
	let error = $state<string | null>(null);

	const toggleMonitor = async () => {
		if (!connected || transition) return;
		error = null;
		const starting = !enabled;
		if (starting && !sourceName) {
			error = 'Choose a source in the main app';
			return;
		}
		transition = starting ? 'starting' : 'stopping';
		try {
			if (starting) await backend.startMonitor(sourceName!);
			else await backend.stopMonitor();
		} catch (cause) {
			error = cause instanceof Error ? cause.message : String(cause);
		} finally {
			transition = null;
		}
	};

	const handleKeydown = (event: KeyboardEvent) => {
		if (event.key.toLowerCase() === 't') {
			event.preventDefault();
			if (event.repeat) return;
			showTimes = !showTimes;
			try {
				localStorage.setItem(timesStorageKey, String(showTimes));
			} catch {
				/* Keep the toggle usable when storage is unavailable. */
			}
			return;
		}
		if (event.key === ' ') {
			event.preventDefault();
			if (!event.repeat) void toggleMonitor();
			return;
		}
		if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return;
		event.preventDefault();
		fontSize = clampSize(fontSize + (event.key === 'ArrowUp' ? 1 : -1));
		try {
			localStorage.setItem(storageKey, String(fontSize));
		} catch {
			/* Storage may be unavailable in OBS. */
		}
	};

	onMount(() => {
		try {
			const saved = Number(localStorage.getItem(storageKey));
			if (Number.isFinite(saved) && saved > 0) fontSize = clampSize(saved);
		} catch {
			/* Keep the default when storage is unavailable. */
		}

		try {
			showTimes = localStorage.getItem(timesStorageKey) !== 'false';
		} catch {
			/* Keep times visible when storage is unavailable. */
		}

		let stopped = false;
		let socket: WebSocket | null = null;
		let reconnect: ReturnType<typeof setTimeout> | undefined;
		const receive = (event: AppEvent) => {
			if (event.type === 'snapshot') {
				connected = true;
				enabled = event.state.monitor.enabled;
				sourceName =
					event.state.monitor.sourceName ?? event.state.settingsStatus?.settings.lastUsedSourceName ?? sourceName;
				if (enabled) error = null;
				recordingState = event.state.recordingState;
				times = event.state.match?.times ?? null;
			} else if (event.type === 'monitorStopped') {
				enabled = false;
				times = null;
				recordingState = null;
			} else if (event.type === 'recordingSaved' && recordingState !== 'started') {
				recordingState = null;
			}
		};
		const connect = () => {
			socket = backend.connectAppSocket(receive, () => {
				connected = false;
				times = null;
				if (!stopped) reconnect = setTimeout(connect, 1000);
			});
		};
		connect();
		return () => {
			stopped = true;
			clearTimeout(reconnect);
			socket?.close();
		};
	});
</script>

<svelte:head><title>The Golden Eye — Minimal overlay</title></svelte:head>
<svelte:window onkeydown={handleKeydown} />
<MinimalOverlay {connected} {enabled} {recordingState} {times} {showTimes} {fontSize} {transition} {error} />
