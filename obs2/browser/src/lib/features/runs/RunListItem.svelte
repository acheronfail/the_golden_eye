<script lang="ts">
	import type { RunClip } from '$lib/api';
	import ActionMenu, { type ActionMenuItem } from '$lib/ui/ActionMenu.svelte';
	import Tooltip from '$lib/ui/Tooltip.svelte';
	import {
		formatDate,
		formatRunListDate,
		isCompleted,
		gameLanguageLabel,
		statusLabel,
		wasPersonalBest
	} from '$lib/features/runs/runsView';
	import { settings } from '$lib/stores/settings.svelte';

	let {
		clip,
		showDate = false,
		busy = false,
		menuOpen = $bindable(false),
		onMenuOpenChange,
		fileBrowserLabel,
		open,
		rename,
		reveal,
		remove,
		keep = () => {}
	}: {
		clip: RunClip;
		showDate?: boolean;
		busy?: boolean;
		menuOpen?: boolean;
		onMenuOpenChange?: (open: boolean) => void;
		fileBrowserLabel: string;
		open: (clip: RunClip) => void;
		rename: (clip: RunClip) => void | Promise<void>;
		reveal: (clip: RunClip) => void | Promise<void>;
		remove: (clip: RunClip) => void | Promise<void>;
		keep?: (clip: RunClip) => void | Promise<void>;
	} = $props();

	const retentionLabelFor = (
		run: RunClip
	): {
		label: string;
		description: string;
	} => {
		if (!run.path) {
			switch (run.retentionReason) {
				case 'manualEntry':
					return {
						label: 'manual',
						description: 'This entry was manually added.'
					};
				case 'theElite':
					return {
						label: 'the elite',
						description: 'This entry was imported and downloaded from the-elite.net.'
					};
				default:
					return {
						label: 'history only',
						description: 'This entry has no local video clip.'
					};
			}
		}

		switch (run.retentionState) {
			case 'pending':
				return {
					label: 'pending',
					description: `This entry has a local video clip, but unless you choose to keep it, it will be deleted if ${settings.values.recentRunLimit} newer clips are saved.`
				};
			case 'expired':
				return {
					label: 'expired',
					description: 'This entry expired and so its clip has been deleted.'
				};
			case 'kept':
				return {
					label: 'kept',
					description: "This entry has been explicitly kept, and won't be deleted."
				};
		}
	};

	const compactStatusLabelFor = (run: RunClip, isPersonalBest: boolean): string => {
		if (isPersonalBest) return 'PB';
		if (run.metadata.status === 'kia') return 'Killed';
		return statusLabel(run.metadata.status) || 'unknown';
	};

	const statusDotClass = (isPersonalBest: boolean, isCompleted: boolean, isFailed: boolean): string => {
		if (isPersonalBest) return 'bg-(--obs-gold)';
		if (isCompleted) return 'bg-(--obs-success)';
		if (isFailed) return 'bg-(--obs-danger)';
		return 'bg-(--obs-text-dim)';
	};

	const actionItems = $derived<ActionMenuItem[]>([
		{ label: 'Open', action: () => open(clip) },
		...(clip.path ? [{ label: 'Rename', action: () => rename(clip) }] : []),
		...(clip.path ? [{ label: fileBrowserLabel, action: () => reveal(clip) }] : []),
		...(clip.path && clip.retentionState === 'pending' ? [{ label: 'Keep', action: () => keep(clip) }] : []),
		{ label: 'Delete', action: () => remove(clip), tone: 'danger' }
	]);
	const completed = $derived(isCompleted(clip));
	const failed = $derived(['failed', 'abort', 'kia'].includes(clip.metadata.status));
	const personalBest = $derived(wasPersonalBest(clip));
	const pending = $derived(Boolean(clip.path && clip.retentionState === 'pending'));
	const levelName = $derived(clip.metadata.level || 'unknown');
	const retentionLabel = $derived(retentionLabelFor(clip));
	const itemLabel = $derived(clip.fileName ? `Open ${clip.fileName}` : `Open ${levelName} run history only`);
	const timestampLabel = $derived(formatRunListDate(clip.metadata.timestamp, showDate));
	const timestampTitle = $derived(formatDate(clip.metadata.timestamp));
	const compactStatusLabel = $derived(compactStatusLabelFor(clip, personalBest));
</script>

<div
	class="relative grid grid-cols-[minmax(0,1fr)_auto] border-b border-(--obs-border-muted) px-2 transition-colors hover:bg-(--obs-control-hover)"
>
	<button
		type="button"
		class="grid min-h-14 min-w-0 cursor-pointer grid-cols-[minmax(5.5rem,1fr)_minmax(5rem,.8fr)_minmax(3.75rem,.6fr)_minmax(4rem,.6fr)] items-center gap-2 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-(--obs-gold-hover) sm:grid-cols-[minmax(8rem,1.35fr)_minmax(6rem,.8fr)_minmax(5.5rem,.7fr)_minmax(4.5rem,.6fr)]"
		aria-label={itemLabel}
		onclick={() => open(clip)}
	>
		<span class="flex min-w-0 flex-col">
			<strong class="truncate text-sm font-medium">{levelName}</strong>
			<span class="truncate font-mono text-[10px] text-(--obs-text-dim)" title={timestampTitle}>
				{#if gameLanguageLabel(clip.metadata.gameLanguage)}{gameLanguageLabel(clip.metadata.gameLanguage)} ·
				{/if}{timestampLabel}
			</span>
		</span>

		<span class="flex min-w-0 flex-col">
			<strong class="font-mono text-sm font-semibold tabular-nums" class:text-(--obs-gold-hover)={personalBest}
				>{clip.metadata.time || '—'}</strong
			>
			<span class="truncate text-[10px] text-(--obs-text-dim)">{clip.metadata.difficulty || '—'}</span>
		</span>
		<span class="flex min-w-0 items-center gap-1.5 truncate font-mono text-[10px]">
			<span class="size-1.5 shrink-0 rounded-full {statusDotClass(personalBest, completed, failed)}" aria-hidden="true"
			></span>
			{compactStatusLabel}
		</span>
		<Tooltip content={retentionLabel.description} class="cursor-help">
			<span
				class="truncate font-mono text-[10px] {pending ? 'font-semibold text-(--obs-danger)' : 'text-(--obs-text-dim)'}"
			>
				{retentionLabel.label}
			</span>
		</Tooltip>
	</button>

	<ActionMenu
		items={actionItems}
		label="More actions"
		title={`Actions for ${clip.fileName || `${clip.metadata.level} run`}`}
		{busy}
		bind:open={menuOpen}
		onOpenChange={onMenuOpenChange}
		triggerClass="h-8 w-8 self-center rounded px-2 font-mono text-lg"
	/>
</div>
