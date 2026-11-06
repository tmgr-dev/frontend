export const CLOUD_SEEN_KEY = 'tmgr:cloud-seen';

export const markCloudSeen = (storage: Pick<Storage, 'setItem'>): void => {
	try {
		storage.setItem(CLOUD_SEEN_KEY, '1');
	} catch {
		/* storage unavailable: the install may look fresh */
	}
};

export const hasCloudSeen = (storage: Pick<Storage, 'getItem'>): boolean => {
	try {
		return storage.getItem(CLOUD_SEEN_KEY) !== null;
	} catch {
		return true;
	}
};
