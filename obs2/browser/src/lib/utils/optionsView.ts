import type { FolderValidation } from '$lib/api';

export type OptionsPathKind = 'completed' | 'failed';

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
		picking: OptionsPathKind | null;
		completed: {
			validating: boolean;
			validation: FolderValidation | null;
			placeholder: string;
		};
		failed: {
			validating: boolean;
			validation: FolderValidation | null;
			placeholder: string;
		};
		choose: (kind: OptionsPathKind) => void;
		clear: (kind: OptionsPathKind) => void;
		clearValidation: (kind: OptionsPathKind) => void;
		validate: (kind: OptionsPathKind) => void;
		statusMessage: (validation: FolderValidation) => string;
	};
	normalize: {
		preRunPadding: () => void;
		postRunPadding: () => void;
	};
}
