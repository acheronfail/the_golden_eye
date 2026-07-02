<script lang="ts">
	import { dismissNotificationFlag, notifications, type NotificationTone } from './notifications.svelte';

	const toneClass = (tone: NotificationTone): string => {
		switch (tone) {
			case 'success':
				return 'border-emerald-400 text-emerald-300';
			case 'warning':
				return 'border-amber-400 text-amber-300';
			case 'error':
				return 'border-red-400 text-red-300';
			case 'info':
			default:
				return 'border-cyan-400 text-cyan-300';
		}
	};
</script>

{#if notifications.flags.length > 0}
	<div
		class="pointer-events-none fixed top-24 right-4 z-50 flex w-[min(28rem,calc(100vw-2rem))] flex-col items-end gap-2"
		aria-live="polite"
		aria-atomic="false"
	>
		{#each notifications.flags as flag (flag.id)}
			<div
				class="pointer-events-auto w-full border-l-4 bg-neutral-950/95 px-4 py-3 text-left font-mono shadow-lg ring-1 ring-neutral-700/80 {toneClass(
					flag.tone
				)}"
				role="status"
			>
				<div class="flex min-w-0 items-start gap-3">
					<div class="min-w-0 flex-1">
						<p class="text-xs tracking-widest uppercase">{flag.title}</p>
						{#if flag.detail}
							<p class="mt-1 text-xs break-all text-neutral-200">{flag.detail}</p>
						{/if}
						{#if flag.meta}
							<p class="mt-1 text-xs text-neutral-500">{flag.meta}</p>
						{/if}
					</div>
					<button
						type="button"
						class="shrink-0 text-xs text-neutral-500 hover:text-neutral-100 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-neutral-400"
						aria-label="Dismiss notification"
						onclick={() => dismissNotificationFlag(flag.id)}
					>
						x
					</button>
				</div>
			</div>
		{/each}
	</div>
{/if}
