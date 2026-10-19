export interface AutopilotInput {
	running: boolean;
	pid: number | null;
	status: StatusDoc | null;
}

export interface ViewOptions {
	tzMinutes?: number;
}

export interface SlotsDoc {
	running?: number;
	max?: number;
}

export interface TaskCountsDoc {
	total?: number;
	done_pr?: number;
	in_work?: number;
	needs_feedback?: number;
	blocked?: number;
	held?: number;
	failed?: number;
	skipped?: number;
	queued?: number;
}

export interface SettingsDoc {
	usage_guard?: boolean;
	feedback_status?: string;
	blocked_status?: string;
	[key: string]: unknown;
}

export interface RunDoc {
	task?: string;
	task_id?: number;
	title?: string;
	mode?: string;
	started_at?: string | null;
	repos?: string[];
	step?: string;
	workspace_code?: string;
	workspace_id?: number;
	session_id?: string;
	agents?: unknown;
	[key: string]: unknown;
}

export interface LimitDoc {
	tool?: string;
	window?: string;
	used_pct?: number | null;
	resets_at?: string | null;
	source?: string;
	origin?: string | null;
	observed_at?: string | null;
	stop_new_at?: number | null;
	pause_at?: number | null;
	[key: string]: unknown;
}

export interface CostDoc {
	subscription_usd?: number;
	subscription_tokens?: number;
	api_usd?: number | null;
	api_cap_usd?: number | null;
	billing?: string;
	basis?: string;
	[key: string]: unknown;
}

export interface NextStopDoc {
	window?: string;
	pct_left?: number;
	kind?: string;
	[key: string]: unknown;
}

export interface PrDoc {
	task?: string;
	url?: string;
	draft?: boolean;
	[key: string]: unknown;
}

export interface EventDoc {
	at?: string | null;
	kind?: string;
	task?: string | null;
	text?: string;
	url?: string;
	[key: string]: unknown;
}

export interface ReasonDoc {
	task?: string;
	reason?: string;
	[key: string]: unknown;
}

export interface ReasonGroupDoc {
	held?: ReasonDoc[];
	failed?: ReasonDoc[];
	[key: string]: unknown;
}

export interface StatusDoc {
	version?: number;
	state?: string;
	started_at?: string;
	updated_at?: string;
	rule?: string;
	mode?: string;
	stop_reason?: string | null;
	settings?: SettingsDoc;
	tasks?: TaskCountsDoc;
	teams?: SlotsDoc;
	agents?: SlotsDoc;
	runs?: RunDoc[];
	limits?: LimitDoc[];
	cost?: CostDoc;
	next_stop?: NextStopDoc | null;
	unconfirmed_new?: number;
	prs?: PrDoc[];
	events?: EventDoc[];
	kind?: string;
	summary?: ReasonGroupDoc;
	resume?: ReasonGroupDoc;
	[key: string]: unknown;
}

export interface HeaderView {
	title: 'Autopilot' | 'Queue loop';
	word: string;
	note: string | null;
	needsUpdate: boolean;
	startedAt: string | null;
	runningFor: string | null;
	rule: string | null;
	mode: string | null;
	stopReason: string | null;
	stale: boolean;
	agents: string | null;
	teams: string | null;
}

export interface ButtonsView {
	pause: boolean;
	resume: boolean;
	stop: boolean;
}

export interface CounterItem {
	task: string;
	title?: string;
	url?: string;
	reason?: string;
}

export interface CounterView {
	key:
		| 'done'
		| 'inWork'
		| 'needsFeedback'
		| 'blocked'
		| 'held'
		| 'failed'
		| 'queued';
	label: string;
	count: number;
	items: CounterItem[] | null;
}

export interface RunView {
	task: string;
	taskId: number | null;
	title: string;
	mode: string;
	duration: string;
	repos: string[];
	step: string;
	agents?: string;
	workspaceCode?: string;
	sessionId?: string;
	prUrl?: string;
}

export interface LimitView {
	label: string;
	text: string;
	usedPct: number | null;
	source: string;
	soft: number | null;
	hard: number | null;
	resets: string | null;
	age: string | null;
}

export interface CostView {
	subscription: string;
	basis: string | null;
	api: string;
	tokens: number;
}

export interface EventView {
	at: string;
	kind: string;
	task: string | null;
	text: string;
	url?: string;
}

export interface AutopilotView {
	empty: boolean;
	emptyText: string;
	emptyCommand: string;
	header: HeaderView | null;
	buttons: ButtonsView;
	counters: CounterView[];
	runs: RunView[];
	limits: LimitView[];
	limitsNote: string | null;
	nextStop: string | null;
	cost: CostView | null;
	unconfirmedNew: number;
	events: EventView[];
}
