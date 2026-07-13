import { getReplayBufferStatus, type ReplayBufferStatus } from './api';

/** Shared, reactive replay-buffer status (what clips are saved from). The root
 * layout polls it on navigation and bounces to `/` while unusable. `status` is
 * `null` until the first check resolves, or when a check fails. */
export const replayBuffer = $state<{ status: ReplayBufferStatus | null; loaded: boolean }>({
	status: null,
	loaded: false
});

/** Whether the replay buffer is *known* to be usable. An unknown status (not yet
 * loaded, or the check failed) counts as unavailable, keeping the wizard gated. */
export const isReplayBufferAvailable = (): boolean => replayBuffer.status?.available === true;
export const isReplayBufferEnabled = isReplayBufferAvailable;

/** Re-query the backend for the current replay-buffer status. */
export const refreshReplayBuffer = async (): Promise<void> => {
	try {
		replayBuffer.status = await getReplayBufferStatus();
	} catch {
		replayBuffer.status = null;
	} finally {
		replayBuffer.loaded = true;
	}
};
