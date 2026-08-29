<script lang="ts">
	import { settings } from '$lib/stores/settings.svelte';
	import { optionsClasses as styles } from '$lib/features/options/optionsView';
	import Tooltip from '$lib/ui/Tooltip.svelte';

	const notificationTemplateTokens = [
		{ value: '{broadcast_url}', description: 'YouTube broadcast URL for the stream.' },
		{ value: '{timestamp}', description: 'ISO timestamp in UTC for when the stream event was handled.' },
		{ value: '{timestamp_local}', description: 'ISO timestamp in local time for when the stream event was handled.' },
		{ value: '{unix_seconds}', description: 'Unix timestamp in seconds, suitable for Discord timestamp markup.' }
	];
</script>

<section class={styles.panel}>
	<label class="flex items-center gap-3">
		<input
			type="checkbox"
			bind:checked={settings.values.discordNotificationsEnabled}
			class="obs-checkbox rounded disabled:cursor-not-allowed disabled:opacity-50"
		/>
		<span class={styles.label}>Enable Discord notifications</span>
	</label>
	<p class={styles.hint}>Enable notifications in Discord for streaming, requires a webhook URL.</p>
</section>

{#if settings.values.discordNotificationsEnabled}
	<section class={styles.panel}>
		<label class={styles.label} for="discord-webhook-url">Discord webhook URL</label>
		<input
			id="discord-webhook-url"
			type="url"
			bind:value={settings.values.discordWebhookUrl}
			placeholder="https://discord.com/api/webhooks/..."
			autocomplete="off"
			spellcheck="false"
			class={styles.input}
		/>
	</section>

	<section class={styles.panel}>
		<label class={styles.label} for="streaming-started-message-template">Streaming started message template</label>
		<textarea
			id="streaming-started-message-template"
			rows="3"
			bind:value={settings.values.streamingStartedMessageTemplate}
			placeholder={settings.defaults.streamingStartedMessageTemplate}
			class={styles.textarea}
		></textarea>
		<p class={styles.hint}>Available tokens</p>
		<div class="flex flex-wrap gap-2">
			{#each notificationTemplateTokens as token}
				<Tooltip content={token.description} class="cursor-help">
					<code class={styles.templateToken} aria-label={`${token.value}: ${token.description}`}>{token.value}</code>
				</Tooltip>
			{/each}
		</div>
	</section>

	<section class={styles.panel}>
		<label class={styles.label} for="streaming-stopped-message-template">Streaming stopped message template</label>
		<textarea
			id="streaming-stopped-message-template"
			rows="3"
			bind:value={settings.values.streamingStoppedMessageTemplate}
			placeholder={settings.defaults.streamingStoppedMessageTemplate}
			class={styles.textarea}
		></textarea>
		<p class={styles.hint}>Available tokens</p>
		<div class="flex flex-wrap gap-2">
			{#each notificationTemplateTokens as token}
				<Tooltip content={token.description} class="cursor-help">
					<code class={styles.templateToken} aria-label={`${token.value}: ${token.description}`}>{token.value}</code>
				</Tooltip>
			{/each}
		</div>
	</section>
{/if}
