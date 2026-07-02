import { getMonitorStatus, type MonitorStatus } from './api';

/**
 * Shared, reactive monitor status. The root layout refreshes this on navigation
 * so global UI can show when monitoring is active, while the monitor page updates
 * it immediately after start/stop actions.
 */
export const monitor = $state<{ status: MonitorStatus | null; loaded: boolean }>({
	status: null,
	loaded: false
});

export const monitorHref = (status: MonitorStatus | null = monitor.status): string | null => {
	if (!status?.enabled) return null;
	return `/source/${encodeURIComponent(status.sourceName)}/${encodeURIComponent(status.lang)}`;
};

export const setMonitorRunning = (sourceName: string, lang: string): void => {
	monitor.status = { enabled: true, sourceName, lang };
	monitor.loaded = true;
};

export const setMonitorStopped = (): void => {
	monitor.status = { enabled: false };
	monitor.loaded = true;
};

/** Re-query the backend for the current monitor status. */
export const refreshMonitor = async (): Promise<MonitorStatus> => {
	try {
		monitor.status = await getMonitorStatus();
		return monitor.status;
	} catch (err) {
		monitor.status = null;
		throw err;
	} finally {
		monitor.loaded = true;
	}
};
