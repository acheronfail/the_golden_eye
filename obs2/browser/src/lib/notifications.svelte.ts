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

export const dismissNotificationFlag = (id: number): void => {
	const timeout = timeouts.get(id);
	if (timeout) {
		clearTimeout(timeout);
		timeouts.delete(id);
	}
	notifications.flags = notifications.flags.filter((flag) => flag.id !== id);
};

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

	if (timeoutMs !== undefined) {
		const timeout = setTimeout(() => {
			dismissNotificationFlag(flag.id);
		}, timeoutMs);
		timeouts.set(flag.id, timeout);
	}

	return flag;
};
