import $axios from '@/plugins/axios';
import {
	absoluteLinkUrl,
	createSignedLinkCache,
} from '@/utils/signedFileLinks';

/** Thrown when a user simply has no picture, so one missing avatar cannot latch signing off. */
class NoAvatar extends Error {}

const links = createSignedLinkCache({
	sign: async (userId: number) => {
		try {
			const {
				data: { data },
			} = await $axios.get(`/users/${userId}/avatar/signed-url`);

			return {
				url: absoluteLinkUrl(data.url, import.meta.env.VITE_API_BASE_URL),
				expiresAt: Date.parse(data.expires_at),
			};
		} catch (error) {
			const status = (error as { response?: { status?: number } })?.response
				?.status;

			// 503: this deployment does not sign links at all - stop asking.
			if (status === 503) {
				return null;
			}

			throw status === 404 ? new NoAvatar() : error;
		}
	},
});

/**
 * TM-142 — a URL an `<img>` can load for a user's picture, or null when there is none (or when the
 * deployment does not sign links). Signatures are cached and re-issued shortly before they expire.
 */
export const avatarUrl = async (userId: number): Promise<string | null> => {
	try {
		return await links.get(userId);
	} catch (error) {
		if (error instanceof NoAvatar) {
			return null;
		}

		throw error;
	}
};

/** Call after the picture changes, so the next read mints a link to the new object. */
export const forgetAvatar = (userId: number): void => links.invalidate(userId);

export const storeAvatar = async (path: string): Promise<void> => {
	await $axios.post('/user/store-avatar', { path });
};

export const removeAvatar = async (): Promise<void> => {
	await $axios.delete('/user/avatar');
};
