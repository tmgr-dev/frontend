export const CLOUD_SEEN_KEY = 'tmgr:cloud-seen';

export const hasCloudSeen = (storage: Pick<Storage, 'getItem'>): boolean => {
	try {
		return storage.getItem(CLOUD_SEEN_KEY) !== null;
	} catch {
		return true;
	}
};

const RAN_BEFORE_KEY = 'desktop.whatsNew.lastSeenVersion';

export const hasRunBefore = (storage: Pick<Storage, 'getItem'>): boolean => {
	try {
		return storage.getItem(RAN_BEFORE_KEY) !== null;
	} catch {
		return true;
	}
};
