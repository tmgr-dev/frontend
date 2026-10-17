import {
	offerBackups,
	offerMoveToCloud,
	type DesktopOfferEnv,
} from '../desktopOffers';

const memory = () => {
	const data = new Map<string, string>();
	return {
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
	};
};

const env = (
	over: Partial<DesktopOfferEnv> = {},
): DesktopOfferEnv & { offer: jest.Mock } => {
	const offer = jest.fn().mockResolvedValue(true);
	return {
		isDesktop: () => true,
		windowLabel: () => 'main',
		storage: memory(),
		offer,
		hasLocalWorkspaces: async () => true,
		...over,
	} as DesktopOfferEnv & { offer: jest.Mock };
};

describe('desktop offers', () => {
	const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

	it('offers backups once per install', async () => {
		const e = env();
		offerBackups(e);
		offerBackups(e);
		await flush();
		offerBackups(e);
		expect(e.offer).toHaveBeenCalledTimes(1);
		expect(e.offer).toHaveBeenCalledWith('desktop.backups', 'event', {
			retryMs: 60_000,
		});
	});

	it('offers move to cloud once when a local workspace exists', async () => {
		const e = env();
		await Promise.all([offerMoveToCloud(e), offerMoveToCloud(e)]);
		await offerMoveToCloud(e);
		expect(e.offer).toHaveBeenCalledTimes(1);
		expect(e.offer).toHaveBeenCalledWith('desktop.move-to-cloud', 'event', {
			retryMs: 60_000,
		});
	});

	it('does not offer move to cloud without local workspaces, and retries later', async () => {
		let has = false;
		const e = env({ hasLocalWorkspaces: async () => has });
		await offerMoveToCloud(e);
		expect(e.offer).not.toHaveBeenCalled();
		has = true;
		await offerMoveToCloud(e);
		expect(e.offer).toHaveBeenCalledTimes(1);
	});

	it('writes no flag while the offer is not shown, so the next trigger offers again', async () => {
		const e = env();
		e.offer.mockResolvedValueOnce(false).mockResolvedValue(true);
		await offerMoveToCloud(e);
		expect(
			e.storage.getItem('tmgr:tours:offered:desktop.move-to-cloud'),
		).toBeNull();
		await offerMoveToCloud(e);
		expect(e.offer).toHaveBeenCalledTimes(2);
		expect(e.storage.getItem('tmgr:tours:offered:desktop.move-to-cloud')).toBe(
			'1',
		);
		await offerMoveToCloud(e);
		expect(e.offer).toHaveBeenCalledTimes(2);
	});

	it('does not offer backups again after a failed offer is dropped and keeps no flag', async () => {
		const e = env();
		e.offer.mockRejectedValueOnce(new Error('x'));
		offerBackups(e);
		await flush();
		expect(e.storage.getItem('tmgr:tours:offered:desktop.backups')).toBeNull();
		offerBackups(e);
		await flush();
		expect(e.offer).toHaveBeenCalledTimes(2);
	});

	it('stays silent on the web and in detached windows', async () => {
		for (const [desktop, label] of [
			[false, 'main'],
			[true, 'quick-add'],
			[true, null],
		] as const) {
			const e = env({ isDesktop: () => desktop, windowLabel: () => label });
			offerBackups(e);
			await offerMoveToCloud(e);
			expect(e.offer).not.toHaveBeenCalled();
		}
	});

	it('survives a broken storage and a failing workspace lookup', async () => {
		const broken = {
			getItem: () => {
				throw new Error('x');
			},
			setItem: () => {
				throw new Error('x');
			},
		};
		const e = env({ storage: broken });
		expect(() => offerBackups(e)).not.toThrow();
		const f = env({
			hasLocalWorkspaces: async () => {
				throw new Error('x');
			},
		});
		await expect(offerMoveToCloud(f)).resolves.toBeUndefined();
		expect(f.offer).not.toHaveBeenCalled();
	});
});
