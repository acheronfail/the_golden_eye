<script lang="ts">
	import { goto } from '$app/navigation';
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

	const activate = (flag: { href?: string }): void => {
		if (flag.href) void goto(flag.href);
	};
</script>

{#if notifications.flags.length > 0}
	<div
		class="pointer-events-none fixed bottom-4 left-1/2 z-50 flex w-[min(28rem,calc(100vw-2rem))] -translate-x-1/2 flex-col items-center gap-2 sm:right-4 sm:left-auto sm:translate-x-0 sm:items-end"
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
					{#if flag.href}
						<button
							type="button"
							class="min-w-0 flex-1 border-0 bg-transparent p-0 text-left font-mono text-inherit"
							onclick={() => activate(flag)}
						>
							<p class="text-xs tracking-widest uppercase">{flag.title}</p>
							{#if flag.detail}
								<p class="obs-muted mt-1 text-xs break-all">{flag.detail}</p>
							{/if}
							{#if flag.meta}
								<p class="obs-dim mt-1 text-xs">{flag.meta}</p>
							{/if}
						</button>
					{:else}
						<div class="min-w-0 flex-1">
							<p class="text-xs tracking-widest uppercase">{flag.title}</p>
							{#if flag.detail}
								<p class="obs-muted mt-1 text-xs break-all">{flag.detail}</p>
							{/if}
							{#if flag.meta}
								<p class="obs-dim mt-1 text-xs">{flag.meta}</p>
							{/if}
						</div>
					{/if}
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
