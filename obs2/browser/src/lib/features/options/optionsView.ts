import type { FolderValidation } from '$lib/api';
import { formatDatetimeLocal } from '../youtube/youtubeMetadata';

interface Token {
	value: string;
	description: string;
}

const commonTemplateTokens: Token[] = [
	{ value: '{difficulty}', description: 'Difficulty name: Agent, Secret Agent, 00 Agent, or 007.' },
	{ value: '{level}', description: 'GoldenEye level name, such as Dam, Facility, or Egypt.' },
	{ value: '{levelNumber}', description: 'GoldenEye level number from 1 through 20.' },
	{ value: '{mission}', description: 'GoldenEye mission number from 1 through 9.' },
	{ value: '{part}', description: 'GoldenEye mission part number (roman numeral) from i through v.' },
	{ value: '{plugin_version}', description: 'Current version of this OBS plugin.' },
	{ value: '{rom}', description: 'ROM version: NTSC-U, NTSC-J, PAL, or unknown.' },
	{ value: '{status}', description: 'Run result: complete, failed, abort, or kia.' },
	{ value: '{time}', description: 'Run time as mm:ss when the stats screen was read.' },
	{ value: '{timestamp_local}', description: 'ISO timestamp in local time for when the run completed.' },
	{ value: '{timestamp}', description: 'ISO timestamp in UTC for when the run completed.' }
];

const sortTokens = (a: Token, b: Token) => a.value.localeCompare(b.value);

export const clipTemplateTokens: Token[] = [
	...commonTemplateTokens,
	{ value: '{obs_replay_name}', description: 'Original OBS replay-buffer filename without the extension.' }
].sort(sortTokens);

export const youtubeTemplateTokens: Token[] = [
	...commonTemplateTokens,
	{
		value: '{datetime_local}',
		description: `Time and date in local time, e.g. "${formatDatetimeLocal(new Date().toISOString())}"`
	}
].sort(sortTokens);

export const optionsClasses = {
	panel: 'obs-panel grid gap-3 rounded px-4 py-4',
	label: 'text-sm font-semibold',
	hint: 'obs-dim font-mono text-xs',
	input: 'obs-input font-mono text-sm disabled:cursor-not-allowed disabled:opacity-50',
	textarea: 'obs-input min-h-24 resize-y font-mono text-sm disabled:cursor-not-allowed disabled:opacity-50',
	pathButton: 'obs-button px-3 py-1.5 text-xs whitespace-nowrap disabled:cursor-not-allowed disabled:opacity-50',
	pathStatus: 'text-xs text-(--obs-success)',
	pathPending: 'obs-dim break-all font-mono text-xs',
	pathError: 'wrap-break-word text-xs text-(--obs-danger)',
	templateToken: 'obs-token cursor-help break-all rounded px-1.5 py-1 font-mono text-xs'
} as const;

export interface RecordingOptionsView {
	template: {
		separator: string;
		error: string | null;
		set: (value: string) => void;
	};
	paths: {
		picking: boolean;
		validating: boolean;
		validation: FolderValidation | null;
		placeholder: string;
		choose: () => void;
		clear: () => void;
		clearValidation: () => void;
		validate: () => void;
		statusMessage: (validation: FolderValidation) => string;
	};
	normalize: {
		recentRunLimit: () => void;
		preRunPadding: () => void;
		postRunPadding: () => void;
	};
}
