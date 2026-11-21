export type PopoverTask = {
	id: number;
	workspaceId: number;
	title: string;
	commonTime: number;
	startTime: number;
};

export type PopoverPluginItem = { id: string; title: string };

export type PopoverPluginSection = {
	pluginName: string;
	label: string;
	items: PopoverPluginItem[];
};

export type UsageBar = { pct: number; resetsAt: number | null };

export type PopoverExtras = {
	workspace?: { name: string; code: string };
	todaySecondsByTask?: Record<string, number>;
	today?: {
		trackedSeconds: number;
		routines: { id: number; title: string; done: boolean }[];
		deadlines: {
			id: number;
			workspaceId: number;
			title: string;
			code: string | null;
			dueAt: number;
		}[];
	};
	usage?: {
		fiveHour?: UsageBar;
		week?: UsageBar;
		todayCost?: number;
		todayTokens?: number;
		runningCost?: number;
	};
	machines?: {
		id: string;
		label: string;
		state: 'online' | 'busy' | 'offline';
		sessions: number | null;
		local: boolean;
	}[];
};

export type PopoverToastRecord = {
	id: string;
	kind: 'decision' | 'agent' | 'idle' | 'limit';
	title: string;
	body: string;
	at: number;
};

export type PopoverModel = {
	platform: 'macos' | 'windows' | 'linux';
	version: string;
	now: number;
	running: PopoverTask[];
	recent: PopoverTask[];
	pluginSections: PopoverPluginSection[];
	dnd: { option: 'off' | '1h' | '3h' | 'tomorrow'; until: number | null };
	autostart: boolean;
	updaterEnabled: boolean;
	decisionsAvailable: boolean;
	openDecisions: number;
	extras: PopoverExtras;
	missed: PopoverToastRecord[];
};

export const POPOVER_ACTIONS_FIXED = [
	'open',
	'decisions',
	'manager',
	'shortcuts',
	'autostart',
	'quit',
	'tray_check_for_updates',
	'resume-last',
	'open-usage',
	'open-runner',
	'close',
] as const;
