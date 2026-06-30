import { getReplayBufferStatus, type ReplayBufferStatus } from './api';

/**
 * Shared, reactive replay-buffer status. The whole wizard depends on OBS's
 * replay buffer being enabled (it's what clips are saved from), so the root
 * layout polls this on every navigation and bounces the user back to `/` while
 * it's disabled. `status` is `null` until the first check resolves, or when a
 * check fails (e.g. the backend is unreachable).
 */
export const replayBuffer = $state<{ status: ReplayBufferStatus | null; loaded: boolean }>({
	status: null,
	loaded: false
});

/**
 * Whether the replay buffer is *known* to be enabled. An unknown status (not
 * yet loaded, or the check failed) counts as not-enabled, so the wizard stays
 * gated until we positively confirm it's on.
 */
export const isReplayBufferEnabled = (): boolean => replayBuffer.status?.enabled === true;

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
