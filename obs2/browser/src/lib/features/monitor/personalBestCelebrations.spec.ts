import { expect, it, vi } from 'vitest';
import type { RunClip, RunsResponse } from '$lib/api';
import { PersonalBestCelebrations } from './personalBestCelebrations';

const pb: RunClip = {
	retentionState: 'kept',
	retentionReason: 'personalBest',
	runId: 'pb-1',
	path: '',
	fileName: '',
	directory: '',
	sizeBytes: 0,
	metadata: {
		timestamp: '2026-10-08T00:00:00Z',
		level: 'Dam',
		difficulty: 'Agent',
		status: 'complete',
		timeSeconds: 52,
		wasPersonalBest: true,
		sourceName: 'N64',
		gameLanguage: 'en',
		comment: '',
		pluginVersion: 'test'
	}
};
const response = (run = pb): RunsResponse => ({ clips: [], directories: [], requestedRun: run });
const fixture = () => {
	const getRuns = vi.fn().mockResolvedValue(response());
	const celebrate = vi.fn();
	let source: string | null = 'N64';
	return {
		getRuns,
		celebrate,
		changeSource: (next: string | null) => {
			source = next;
		},
		controller: new PersonalBestCelebrations({ getRuns }, () => source, celebrate)
	};
};

it('celebrates a confirmed PB once across duplicate and concurrent catalog events', async () => {
	const f = fixture();
	await Promise.all([
		f.controller.handle({ runId: 'pb-1', saveId: 1 }),
		f.controller.handle({ runId: 'pb-1', saveId: 1 })
	]);
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	expect(f.celebrate).toHaveBeenCalledTimes(1);
	expect(f.getRuns).toHaveBeenCalledTimes(1);
});

it('celebrates a later PB independently', async () => {
	const f = fixture();
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	f.getRuns.mockResolvedValue(response({ ...pb, runId: 'pb-2' }));
	await f.controller.handle({ runId: 'pb-2', saveId: 2 });
	expect(f.celebrate).toHaveBeenCalledTimes(2);
});

it.each([{ wasPersonalBest: false }, { status: 'failed' }, { sourceName: 'Other' }])(
	'does not celebrate %j',
	async (metadata) => {
		const f = fixture();
		f.getRuns.mockResolvedValue(response({ ...pb, metadata: { ...pb.metadata, ...metadata } }));
		await f.controller.handle({ runId: 'pb-1', saveId: 1 });
		expect(f.celebrate).not.toHaveBeenCalled();
	}
);

it('ignores historical edits, imports, and events outside monitoring', async () => {
	const f = fixture();
	await f.controller.handle({ runId: 'pb-1' });
	await f.controller.handle({ saveId: 1 });
	f.changeSource(null);
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	expect(f.getRuns).not.toHaveBeenCalled();
	expect(f.celebrate).not.toHaveBeenCalled();
});

it.each([null, 'Other'])('ignores a response after the active source changes to %s', async (source) => {
	const f = fixture();
	let resolve!: (value: RunsResponse) => void;
	f.getRuns.mockReturnValue(
		new Promise<RunsResponse>((done) => {
			resolve = done;
		})
	);
	const pending = f.controller.handle({ runId: 'pb-1', saveId: 1 });
	f.changeSource(source);
	resolve(response());
	await pending;
	expect(f.celebrate).not.toHaveBeenCalled();
});

it('allows the later clip completion event to retry a failed catalog request', async () => {
	const f = fixture();
	f.getRuns.mockRejectedValueOnce(new Error('unavailable'));
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	await f.controller.handle({ runId: 'pb-1', saveId: 1 });
	expect(f.celebrate).toHaveBeenCalledTimes(1);
});
