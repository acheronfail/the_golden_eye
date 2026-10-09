import type { LevelMatch, RecordingStatus } from '$lib/api';
import { reconcileMonitorRunIdentity, type MonitorRunIdentity } from './monitorRunIdentity';

export class MonitorRunContext {
	identity = $state<MonitorRunIdentity | null>(null);
	personalBestIdentity = $state<MonitorRunIdentity | null>(null);

	update(match: LevelMatch | null | undefined, recordingState: RecordingStatus | null | undefined): void {
		this.identity = reconcileMonitorRunIdentity(this.identity, match);
		const inactive =
			!recordingState ||
			recordingState === 'cancelled' ||
			recordingState === 'statsSkipped' ||
			recordingState === 'savePending' ||
			match?.screen.trim().toLowerCase() === 'stats';
		this.personalBestIdentity = inactive ? null : reconcileMonitorRunIdentity(this.personalBestIdentity, match);
	}

	reset(): void {
		this.identity = null;
		this.personalBestIdentity = null;
	}
}
