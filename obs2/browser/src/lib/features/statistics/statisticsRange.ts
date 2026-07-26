export type DateRangePreset = 'today' | '7d' | '30d' | '12m' | 'all' | 'custom';

export interface DateRangeSelection {
	preset: DateRangePreset;
	customFrom: string;
	customTo: string;
}

function localStart(date: Date): Date {
	return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function localDateValue(date: Date): string {
	const year = date.getFullYear();
	const month = String(date.getMonth() + 1).padStart(2, '0');
	const day = String(date.getDate()).padStart(2, '0');
	return `${year}-${month}-${day}`;
}

export function isLocalDateValue(value: string): boolean {
	const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
	if (!match) return false;

	const year = Number(match[1]);
	const month = Number(match[2]);
	const day = Number(match[3]);
	const date = new Date(year, month - 1, day);
	return date.getFullYear() === year && date.getMonth() === month - 1 && date.getDate() === day;
}

export function defaultDateRange(now = new Date()): DateRangeSelection {
	return { preset: '30d', customFrom: localDateValue(now), customTo: localDateValue(now) };
}

export function resolveDateRange(
	selection: DateRangeSelection,
	now = new Date()
): { from?: string; to: string; error?: string } {
	const to = now.toISOString();
	if (selection.preset === 'all') return { to };
	if (selection.preset === 'custom') {
		if (!selection.customFrom || !selection.customTo) return { to, error: 'Choose both a start and end date.' };
		if (!isLocalDateValue(selection.customFrom) || !isLocalDateValue(selection.customTo)) {
			return { to, error: 'Choose valid start and end dates.' };
		}
		const fromDate = new Date(`${selection.customFrom}T00:00:00`);
		const inclusiveEnd = new Date(`${selection.customTo}T00:00:00`);
		if (fromDate > inclusiveEnd) return { to, error: 'Start date must not be after end date.' };
		inclusiveEnd.setDate(inclusiveEnd.getDate() + 1);
		return { from: fromDate.toISOString(), to: inclusiveEnd.toISOString() };
	}
	const from = localStart(now);
	if (selection.preset === '7d') from.setDate(from.getDate() - 6);
	if (selection.preset === '30d') from.setDate(from.getDate() - 29);
	if (selection.preset === '12m') from.setFullYear(from.getFullYear() - 1);
	return { from: from.toISOString(), to };
}
