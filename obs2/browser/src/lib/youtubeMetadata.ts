import type { RunClip } from './api';

const dateFromTimestamp = (timestamp: string): Date | null => {
	const date = new Date(timestamp);
	return Number.isNaN(date.getTime()) ? null : date;
};

export const formatDatetimeLocal = (timestamp: string, locale?: Intl.LocalesArgument): string => {
	const date = dateFromTimestamp(timestamp);
	return date ? date.toLocaleString(locale) : timestamp;
};

export const datetimeLocalForClip = (clip: RunClip, locale?: Intl.LocalesArgument): string =>
	formatDatetimeLocal(clip.metadata.timestamp, locale);
