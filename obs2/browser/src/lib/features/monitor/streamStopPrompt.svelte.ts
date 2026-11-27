import { backend } from '$lib/api';

export const streamStopPrompt = $state({ open: false });

export async function stopMonitorAndPromptForStream(): Promise<void> {
	const { promptToStopStream } = await backend.stopMonitor();
	if (promptToStopStream) streamStopPrompt.open = true;
}
