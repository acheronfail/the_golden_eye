import { backend, type PluginUpdate, type UpdatePhase, type UpdateStatus } from './api';
import {
	addNotificationFlag,
	removeNotificationFlag,
	replaceNotificationFlag,
	type NotificationFlag
} from './notifications.svelte';
import type { UpdateButtonPhase } from './optionsView';
import { settings } from './settings.svelte';

const idleStatus = (): UpdateStatus => ({ phase: 'idle', available: null });
const errorMessage = (err: unknown): string => (err instanceof Error ? err.message : String(err));

export const updates = new (class {
	status = $state<UpdateStatus>(idleStatus());
	private availableNotificationId: number | null = null;
	private progressNotificationId: number | null = null;
	private dismissedVersion: string | null = null;
	private pendingAction = $state<UpdatePhase | null>(null);

	buttonPhase: UpdateButtonPhase = $derived.by(() => {
		const phase = this.pendingAction ?? this.status.phase;
		if (phase === 'checking' || phase === 'downloading' || phase === 'applying') return phase;
		if (phase === 'staged') return 'apply';
		if (phase === 'available') return 'download';
		return 'check';
	});

	pending = $derived(
		this.buttonPhase === 'checking' || this.buttonPhase === 'downloading' || this.buttonPhase === 'applying'
	);

	applyStatus(status: UpdateStatus): void {
		const changed =
			status.phase !== this.status.phase || status.available?.latestVersion !== this.status.available?.latestVersion;
		this.status = status;
		this.syncAvailableNotification();
		if (changed) this.syncProgressNotification();
	}

	async refresh(): Promise<void> {
		this.applyStatus(await backend.getUpdateStatus());
	}

	async check(): Promise<void> {
		if (this.pending) return;
		this.pendingAction = 'checking';
		try {
			const { update } = await backend.checkForUpdateNow();
			if (!update) addNotificationFlag({ title: "You're up to date", tone: 'success' });
			await this.refresh();
		} catch (err) {
			await this.reconcileAfterFailure();
			addNotificationFlag({ title: 'Update check failed', detail: errorMessage(err), tone: 'error' });
		} finally {
			this.pendingAction = null;
		}
	}

	async download(): Promise<boolean> {
		if (this.pending) return false;
		this.pendingAction = 'downloading';
		try {
			await backend.downloadUpdateNow();
			await this.refresh();
			return this.status.phase === 'staged';
		} catch (err) {
			await this.reconcileAfterFailure();
			addNotificationFlag({ title: 'Update download failed', detail: errorMessage(err), tone: 'error' });
			return false;
		} finally {
			this.pendingAction = null;
		}
	}

	async apply(): Promise<boolean> {
		if (this.pending) return false;
		const previous = this.status;
		this.pendingAction = 'applying';
		this.status = { phase: 'applying', available: this.status.available };
		this.syncAvailableNotification();
		this.syncProgressNotification();
		try {
			await backend.applyUpdateNow();
			return true;
		} catch (err) {
			await this.reconcileAfterFailure(previous);
			addNotificationFlag({ title: 'Could not apply update', detail: errorMessage(err), tone: 'error' });
			return false;
		} finally {
			this.pendingAction = null;
		}
	}

	async install(): Promise<void> {
		if (this.status.phase !== 'staged' && !(await this.download())) return;
		if (this.status.phase === 'staged') await this.apply();
	}

	handleStagingFailed(error: string): void {
		addNotificationFlag({
			key: 'plugin-update-staging-failed',
			title: 'Plugin update failed',
			detail: error,
			tone: 'error',
			sticky: true
		});
	}

	handleApplied(version: string, releaseUrl?: string): void {
		this.removeAvailableNotification();
		this.removeProgressNotification();
		this.status = idleStatus();
		addNotificationFlag({
			key: 'plugin-update-applied',
			title: 'Plugin updated',
			detail: `Now running v${version}`,
			tone: 'success',
			meta: releaseUrl ? 'Click to view the changelog.' : undefined,
			action: releaseUrl
				? () => backend.openUpdateRelease(releaseUrl).catch((err) => console.warn('Failed to open changelog', err))
				: undefined
		});
	}

	private async reconcileAfterFailure(fallback?: UpdateStatus): Promise<void> {
		this.pendingAction = null;
		try {
			await this.refresh();
		} catch {
			if (fallback) this.applyStatus(fallback);
		}
	}

	private syncAvailableNotification(): void {
		const update = this.status.phase === 'available' ? this.status.available : null;
		if (!update || settings.autoUpdateEnabled || this.dismissedVersion === update.latestVersion) {
			this.removeAvailableNotification();
			return;
		}

		const notification = this.availableNotification(update);
		if (this.availableNotificationId !== null && replaceNotificationFlag(this.availableNotificationId, notification)) {
			return;
		}
		this.availableNotificationId = addNotificationFlag(notification).id;
	}

	private syncProgressNotification(): void {
		const version = this.status.available?.latestVersion;
		if (this.status.phase === 'downloading') {
			this.replaceProgress({
				key: 'plugin-update-installing',
				title: 'Downloading update',
				detail: version ? `Downloading and verifying ${version}...` : 'Downloading and verifying the update...',
				tone: 'info',
				sticky: true
			});
		} else if (this.status.phase === 'staged') {
			this.replaceProgress({
				key: 'plugin-update-installing',
				title: 'Update ready',
				detail: 'The verified update is ready to apply.',
				tone: 'success',
				sticky: true,
				action: async () => {
					await this.apply();
				}
			});
		} else if (this.status.phase === 'applying') {
			this.replaceProgress({
				key: 'plugin-update-installing',
				title: 'Applying update',
				detail: 'The plugin will briefly reconnect while the update is installed.',
				tone: 'success',
				sticky: true
			});
		} else {
			this.removeProgressNotification();
		}
	}

	private availableNotification(update: PluginUpdate) {
		return {
			key: 'plugin-update-available',
			title: 'Plugin update available',
			detail: `${update.currentVersion} -> ${update.latestVersion}`,
			meta: 'Click to download and install.',
			tone: 'info' as const,
			sticky: true,
			onDismiss: () => {
				this.availableNotificationId = null;
				this.dismissedVersion = update.latestVersion;
			},
			action: () => {
				this.dismissedVersion = update.latestVersion;
				this.removeAvailableNotification();
				return this.install();
			}
		};
	}

	private replaceProgress(notification: Omit<NotificationFlag, 'id' | 'timeoutMs'> & { sticky?: boolean }): void {
		if (this.progressNotificationId !== null && replaceNotificationFlag(this.progressNotificationId, notification)) {
			return;
		}
		this.progressNotificationId = addNotificationFlag(notification).id;
	}

	private removeAvailableNotification(): void {
		if (this.availableNotificationId === null) return;
		removeNotificationFlag(this.availableNotificationId);
		this.availableNotificationId = null;
	}

	private removeProgressNotification(): void {
		if (this.progressNotificationId === null) return;
		removeNotificationFlag(this.progressNotificationId);
		this.progressNotificationId = null;
	}
})();
