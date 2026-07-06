export type NotificationTone = 'success' | 'info' | 'warning' | 'error';

export interface NotificationFlag {
	id: number;
	title: string;
	detail?: string;
	meta?: string;
	tone: NotificationTone;
	timeoutMs?: number;
}

const DEFAULT_TIMEOUT_MS = 5_000;

export const notifications = $state<{
	flags: NotificationFlag[];
}>({
	flags: []
});

let nextId = 1;
const timeouts = new Map<number, ReturnType<typeof setTimeout>>();

const scheduleNotificationTimeout = (flag: NotificationFlag): void => {
	if (flag.timeoutMs === undefined) return;
	const timeout = setTimeout(() => {
		dismissNotificationFlag(flag.id);
	}, flag.timeoutMs);
	timeouts.set(flag.id, timeout);
};

export const dismissNotificationFlag = (id: number): void => {
	const timeout = timeouts.get(id);
	if (timeout) {
		clearTimeout(timeout);
		timeouts.delete(id);
	}
	notifications.flags = notifications.flags.filter((flag) => flag.id !== id);
};

interface NotificationFlagOptions {
	title: string;
	detail?: string;
	meta?: string;
	tone?: NotificationTone;
	timeoutMs?: number;
	sticky?: boolean;
}

export const addNotificationFlag = (options: {
	title: string;
	detail?: string;
	meta?: string;
	tone?: NotificationTone;
	timeoutMs?: number;
	sticky?: boolean;
}): NotificationFlag => {
	const timeoutMs = options.sticky ? undefined : (options.timeoutMs ?? DEFAULT_TIMEOUT_MS);
	const flag: NotificationFlag = {
		id: nextId++,
		title: options.title,
		detail: options.detail,
		meta: options.meta,
		tone: options.tone ?? 'info',
		timeoutMs
	};

	notifications.flags = [...notifications.flags, flag];

	scheduleNotificationTimeout(flag);

	return flag;
};

export const replaceNotificationFlag = (id: number, options: NotificationFlagOptions): NotificationFlag | null => {
	const existing = notifications.flags.find((flag) => flag.id === id);
	if (!existing) return null;

	const timeout = timeouts.get(id);
	if (timeout) {
		clearTimeout(timeout);
		timeouts.delete(id);
	}

	const timeoutMs = options.sticky ? undefined : (options.timeoutMs ?? DEFAULT_TIMEOUT_MS);
	const flag: NotificationFlag = {
		id,
		title: options.title,
		detail: options.detail,
		meta: options.meta,
		tone: options.tone ?? 'info',
		timeoutMs
	};

	notifications.flags = notifications.flags.map((item) => (item.id === id ? flag : item));
	scheduleNotificationTimeout(flag);

	return flag;
};
