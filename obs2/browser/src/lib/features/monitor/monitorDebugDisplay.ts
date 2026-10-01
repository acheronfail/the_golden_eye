const numberFormat = new Intl.NumberFormat('en-US', {
	useGrouping: false,
	maximumFractionDigits: 2
});

export const formatDebugValue = (input: unknown): string =>
	input == null ? 'null' : typeof input === 'number' ? numberFormat.format(input) : String(input);

export const formatDebugJson = (input: unknown, space?: number): string =>
	JSON.stringify(input, (_key, value) => (typeof value === 'number' ? Number(value.toFixed(2)) : value), space);
