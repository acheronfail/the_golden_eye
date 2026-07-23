<script lang="ts">
	import type { RunClip } from '$lib/api';
	import ActionMenu, { type ActionMenuItem } from '$lib/components/ActionMenu.svelte';
	import { formatDate, formatRunListDate, isCompleted, romLanguageLabel, statusLabel } from '$lib/utils/runsView';

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

	const actionItems = $derived<ActionMenuItem[]>([
		{ label: 'Open', action: () => open(clip) },
		...(clip.path ? [{ label: 'Rename', action: () => rename(clip) }] : []),
		...(clip.path ? [{ label: fileBrowserLabel, action: () => reveal(clip) }] : []),
		...(clip.path && clip.retentionState === 'pending' ? [{ label: 'Keep', action: () => keep(clip) }] : []),
		{ label: 'Delete', action: () => remove(clip), tone: 'danger' }
	]);
	const completed = $derived(isCompleted(clip));
	const failed = $derived(['failed', 'abort', 'kia'].includes(clip.metadata.status));
	const pending = $derived(Boolean(clip.path && clip.retentionState === 'pending'));
	const levelName = $derived(clip.metadata.level || 'unknown');
	const retentionLabel = $derived(!clip.path ? 'history only' : clip.retentionState === 'pending' ? 'pending' : 'kept');
	const itemLabel = $derived(clip.fileName ? `Open ${clip.fileName}` : `Open ${levelName} run history only`);
	const timestampLabel = $derived(formatRunListDate(clip.metadata.timestamp, showDate));
	const timestampTitle = $derived(formatDate(clip.metadata.timestamp));
</script>

<div
	class="relative grid grid-cols-[minmax(0,1fr)_auto] border-b border-(--obs-border-muted) transition-colors hover:bg-(--obs-control-hover)"
>
	<button
		type="button"
		class="grid min-h-14 min-w-0 cursor-pointer grid-cols-[minmax(5.5rem,1fr)_minmax(5rem,.8fr)_minmax(3.75rem,.6fr)_minmax(4rem,.6fr)] items-center gap-2 px-2 py-2 text-left transition-colors focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-(--obs-gold-hover) sm:grid-cols-[minmax(8rem,1.35fr)_minmax(6rem,.8fr)_minmax(5.5rem,.7fr)_minmax(4.5rem,.6fr)]"
		aria-label={itemLabel}
		onclick={() => open(clip)}
	>
		<span class="flex min-w-0 flex-col">
			<strong class="truncate text-sm font-medium">{levelName}</strong>
			<span class="truncate font-mono text-[10px] text-(--obs-text-dim)" title={timestampTitle}>
				{#if romLanguageLabel(clip.metadata.romLanguage)}{romLanguageLabel(clip.metadata.romLanguage)} ·
				{/if}{timestampLabel}
			</span>
		</span>

		<span class="flex min-w-0 flex-col">
			<strong class="font-mono text-sm font-semibold tabular-nums">{clip.metadata.time || '—'}</strong>
			<span class="truncate text-[10px] text-(--obs-text-dim)">{clip.metadata.difficulty || '—'}</span>
		</span>
		<span class="flex min-w-0 items-center gap-1.5 truncate font-mono text-[10px]">
			<span
				class="size-1.5 shrink-0 rounded-full {completed
					? 'bg-(--obs-success)'
					: failed
						? 'bg-(--obs-danger)'
						: 'bg-(--obs-text-dim)'}"
				aria-hidden="true"
			></span>
			{statusLabel(clip.metadata.status) || 'unknown'}
		</span>
		<span
			class="truncate font-mono text-[10px] {pending ? 'font-semibold text-(--obs-danger)' : 'text-(--obs-text-dim)'}"
			>{retentionLabel}</span
		>
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
