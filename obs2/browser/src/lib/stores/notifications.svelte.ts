import type { MetaPill } from '$lib/utils/metaPills';

export type NotificationTone = 'success' | 'info' | 'warning' | 'error';

export interface NotificationFlag {
	id: number;
	key?: string;
	title: string;
	detail?: string;
	meta?: string;
	pills?: MetaPill[];
	tone: NotificationTone;
	timeoutMs?: number;
	href?: string;
	action?: () => void | Promise<void>;
	onDismiss?: () => void;
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
	removeNotificationFlag(id, true);
};

export const removeNotificationFlag = (id: number, notifyDismiss = false): void => {
	const flag = notifications.flags.find((item) => item.id === id);
	const timeout = timeouts.get(id);
	if (timeout) {
		clearTimeout(timeout);
		timeouts.delete(id);
	}
	notifications.flags = notifications.flags.filter((item) => item.id !== id);
	if (notifyDismiss) flag?.onDismiss?.();
};

interface NotificationFlagOptions {
	key?: string;
	title: string;
	detail?: string;
	meta?: string;
	pills?: MetaPill[];
	tone?: NotificationTone;
	timeoutMs?: number;
	sticky?: boolean;
	href?: string;
	action?: () => void | Promise<void>;
	onDismiss?: () => void;
}

export const addNotificationFlag = (options: NotificationFlagOptions): NotificationFlag => {
	const timeoutMs = options.sticky ? undefined : (options.timeoutMs ?? DEFAULT_TIMEOUT_MS);
	const flag: NotificationFlag = {
		id: nextId++,
		key: options.key,
		title: options.title,
		detail: options.detail,
		meta: options.meta,
		pills: options.pills,
		tone: options.tone ?? 'info',
		timeoutMs,
		href: options.href,
		action: options.action,
		onDismiss: options.onDismiss
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
		key: options.key,
		title: options.title,
		detail: options.detail,
		meta: options.meta,
		pills: options.pills,
		tone: options.tone ?? 'info',
		timeoutMs,
		href: options.href,
		action: options.action,
		onDismiss: options.onDismiss
	};

	notifications.flags = notifications.flags.map((item) => (item.id === id ? flag : item));
	scheduleNotificationTimeout(flag);

	return flag;
};

export const dismissNotificationFlagsByKey = (key: string): void => {
	for (const flag of notifications.flags) {
		if (flag.key === key) dismissNotificationFlag(flag.id);
	}
};
