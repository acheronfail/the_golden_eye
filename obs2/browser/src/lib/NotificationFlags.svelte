<script lang="ts">
	import { dismissNotificationFlag, notifications, type NotificationTone } from './notifications.svelte';

	const toneClass = (tone: NotificationTone): string => {
		switch (tone) {
			case 'success':
				return 'obs-notification-success';
			case 'warning':
				return 'obs-notification-warning';
			case 'error':
				return 'obs-notification-error';
			case 'info':
			default:
				return 'obs-notification-info';
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
				class="obs-notification pointer-events-auto relative w-full overflow-hidden px-4 py-3 text-left font-mono {toneClass(
					flag.tone
				)}"
				role="status"
			>
				<div class="flex min-w-0 items-start gap-3">
					<div class="min-w-0 flex-1">
						<p class="text-xs tracking-widest uppercase">{flag.title}</p>
						{#if flag.detail}
							<p class="obs-muted mt-1 text-xs break-all">{flag.detail}</p>
						{/if}
						{#if flag.meta}
							<p class="obs-dim mt-1 text-xs">{flag.meta}</p>
						{/if}
					</div>
					<button
						type="button"
						class="obs-text-button shrink-0 px-1.5 py-0.5 text-xs"
						aria-label="Dismiss notification"
						onclick={() => dismissNotificationFlag(flag.id)}
					>
						x
					</button>
				</div>
				{#if flag.timeoutMs !== undefined}
					<div
						class="obs-notification-timeout-bar"
						style={`animation-duration: ${flag.timeoutMs}ms;`}
						aria-hidden="true"
					></div>
				{/if}
			</div>
		{/each}
	</div>
{/if}
