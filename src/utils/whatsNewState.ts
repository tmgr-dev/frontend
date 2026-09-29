import { ref } from 'vue';
import {
	ChangelogSection,
	parseChangelog,
	recentSections,
	whatsNewSections,
} from './whatsNew';

const LAST_SEEN_KEY = 'desktop.whatsNew.lastSeenVersion';

export const whatsNewState = ref<{
	open: boolean;
	sections: ChangelogSection[];
	version: string;
}>({ open: false, sections: [], version: '' });

const readStorage = (key: string): string | null => {
	try {
		return localStorage.getItem(key);
	} catch {
		return null;
	}
};

const writeStorage = (key: string, value: string) => {
	try {
		localStorage.setItem(key, value);
	} catch {
		return;
	}
};

const loadChangelog = async (): Promise<ChangelogSection[]> => {
	const { default: markdown } = await import(
		'../../src-tauri/CHANGELOG.md?raw'
	);
	return parseChangelog(markdown);
};

const currentVersion = async (): Promise<string> => {
	const { getVersion } = await import('@tauri-apps/api/app');
	return getVersion();
};

export const openWhatsNew = async () => {
	try {
		const version = await currentVersion();
		const sections = recentSections(await loadChangelog(), version);
		whatsNewState.value = { open: true, sections, version };
	} catch {
		return;
	}
};

export const closeWhatsNew = () => {
	const { version } = whatsNewState.value;
	whatsNewState.value = { open: false, sections: [], version };
	if (version) writeStorage(LAST_SEEN_KEY, version);
};

export const hadSessionAtLaunch = (): boolean => !!readStorage('token');

export const checkWhatsNewOnStartup = async (hasPriorData: boolean) => {
	try {
		const version = await currentVersion();
		const sections = whatsNewSections(await loadChangelog(), {
			lastSeen: readStorage(LAST_SEEN_KEY),
			current: version,
			hasPriorData,
		});
		if (sections.length === 0) {
			writeStorage(LAST_SEEN_KEY, version);
			return;
		}
		whatsNewState.value = { open: true, sections, version };
	} catch {
		return;
	}
};
