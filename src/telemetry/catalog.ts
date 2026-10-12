// Copy of the server catalog (telemetry/catalog.json); the server drops anything not listed here.
export const CATALOG = {
	"enums": {
		"platform": [
			"web",
			"mobile_web",
			"desktop_mac",
			"desktop_win",
			"desktop_linux",
			"ios"
		],
		"account_mode": [
			"cloud",
			"local",
			"none"
		],
		"viewport": [
			"s",
			"m",
			"l"
		],
		"tour": [
			"start.nav",
			"start.first-task",
			"start.connect-agent",
			"start.hub",
			"tasks.anatomy",
			"tasks.board-move",
			"tasks.statuses",
			"tasks.categories",
			"tasks.comments-files",
			"tasks.relations",
			"tasks.archive",
			"tasks.files-page",
			"tasks.multi-select",
			"time.timer",
			"time.parallel",
			"time.checkpoints",
			"time.pomodoro",
			"time.export",
			"time.timer-prompt",
			"agents.work-log",
			"agents.phone-alerts",
			"agents.mcp-token",
			"agents.ask-ai",
			"agents.connect-later",
			"personas.create",
			"personas.token",
			"personas.assign",
			"personas.skills",
			"pages.first",
			"pages.context",
			"pages.links",
			"pages.versions",
			"pages.people-meetings",
			"pages.import",
			"pages.task-from-selection",
			"graph.map",
			"graph.enable",
			"routines.first",
			"routines.views",
			"routines.reminders",
			"ws.modules",
			"ws.invite",
			"ws.dashboard",
			"ws.workspaces",
			"ws.appearance",
			"ws.notifications",
			"ws.privacy",
			"ws.telegram",
			"ws.default-landing",
			"desktop.quick-add",
			"desktop.capture",
			"desktop.menu-bar-timer",
			"desktop.windows",
			"desktop.local-workspace",
			"desktop.backups",
			"desktop.local-agent",
			"desktop.plugins",
			"desktop.whats-new",
			"desktop.move-to-cloud",
			"ios.tabs",
			"ios.pomodoro",
			"ios.widgets",
			"ios.controls",
			"ios.watch",
			"ios.siri",
			"ios.share",
			"ios.notifications",
			"ios.alarms",
			"ios.alarm-phone",
			"ios.telegram",
			"ios.face-id",
			"ios.ask-ai"
		],
		"trigger": [
			"first_run",
			"first_visit",
			"hub",
			"feature_enabled",
			"event",
			"whats_new"
		],
		"via": [
			"next",
			"action"
		],
		"dur": [
			"lt_1m",
			"m1_3",
			"m3_10",
			"gt_10m"
		],
		"mode": [
			"auto",
			"hub_only",
			"off"
		],
		"intent": [
			"human",
			"agent",
			"skipped"
		],
		"client": [
			"claude_code",
			"codex",
			"cursor",
			"other"
		],
		"item": [
			"has_task",
			"has_timer_minute",
			"has_board_move",
			"has_routine",
			"has_page",
			"has_context_page",
			"mcp_first_call",
			"agent_run_finished",
			"notify_token",
			"invite_sent",
			"email_verified"
		],
		"feature": [
			"timer_started",
			"board_move",
			"page_created",
			"routine_created",
			"quick_add",
			"export",
			"comment_created",
			"relation_created",
			"persona_created",
			"pomodoro_started",
			"ai_question_sent"
		],
		"route": [
			"AgentNotificationSettings",
			"AgentsGuide",
			"ArchiveTasksList",
			"ArchiveTasksListWithWorkspace",
			"Board",
			"CurrentTasksList",
			"DailyRoutines",
			"Dashboard",
			"DesktopAuthReturn",
			"DesktopAuthTelegram",
			"FallbackBoard",
			"FallbackCategoriesList",
			"FallbackCategoryCreate",
			"FallbackTasksList",
			"ForgetPassword",
			"LinkConfirm",
			"Login",
			"ModuleSettings",
			"NewPassword",
			"NotFound",
			"Notifications",
			"PageWindow",
			"PersonaSettings",
			"PluginSettings",
			"PluginSettingsHelp",
			"Profile",
			"ProjectCategoryChildrenList",
			"PushNotificationsEnableGuide",
			"QuickAdd",
			"Register",
			"Settings",
			"SocialiteProxy",
			"Stats",
			"TaskWindow",
			"TasksCreate",
			"TasksCreateWithProjectCategoryId",
			"TasksEdit",
			"VerifyEmail",
			"Welcome",
			"WorkspaceBoard",
			"WorkspaceCategories",
			"WorkspaceCategory",
			"WorkspaceCategoryChildren",
			"WorkspaceCategoryCreate",
			"WorkspaceCategoryCreateInCategory",
			"WorkspaceCategoryEdit",
			"WorkspaceCategoryListWithStatus",
			"WorkspaceCategoryTask",
			"WorkspaceDashboard",
			"WorkspaceFiles",
			"WorkspaceInvitation",
			"WorkspaceMap",
			"WorkspaceMember",
			"WorkspacePage",
			"WorkspacePageVersions",
			"WorkspacePages",
			"WorkspacePagesTrash",
			"WorkspacePluginPage",
			"WorkspaceSettings",
			"WorkspaceTask",
			"WorkspaceTasksList"
		],
		"kind": [
			"manual",
			"auto",
			"cloud_move"
		],
		"phase": [
			"snapshot",
			"upload",
			"import",
			"verify",
			"publish"
		]
	},
	"steps": {
		"start.nav": [
			"nav.sidebar",
			"nav.workspace-switcher",
			"nav.create-task",
			"nav.user-menu",
			"nav.getting-started",
			"nav.menu-button"
		],
		"start.first-task": [
			"nav.create-task",
			"task-form.title",
			"list.row",
			"list-add"
		],
		"start.connect-agent": [
			"connect.client-tabs",
			"connect.command",
			"connect.status",
			"connect.prompt"
		],
		"start.hub": [
			"hub.checklist",
			"hub.catalog",
			"hub.tips-toggle"
		],
		"tasks.anatomy": [
			"task-form.status",
			"task-form.category",
			"task-form.assignees",
			"task-form.description",
			"task-form.done"
		],
		"tasks.board-move": [
			"board.column",
			"board.card-handle",
			"board.add-in-column"
		],
		"tasks.statuses": [
			"statuses.editor",
			"statuses.add"
		],
		"tasks.categories": [
			"categories.create",
			"categories.row"
		],
		"tasks.comments-files": [
			"task-form.comments",
			"task-form.attachments"
		],
		"tasks.relations": [
			"task-form.relations"
		],
		"tasks.archive": [
			"nav.archive"
		],
		"tasks.files-page": [
			"files.list"
		],
		"tasks.multi-select": [
			"list.select"
		],
		"time.timer": [
			"task-form.timer",
			"nav.active-timers",
			"task-form.estimate",
			"task-timer",
			"s2"
		],
		"time.parallel": [
			"nav.active-timers"
		],
		"time.checkpoints": [
			"task-form.checkpoints"
		],
		"time.pomodoro": [
			"task-form.pomodoro",
			"pomodoro.settings"
		],
		"time.export": [
			"list.select",
			"export.panel",
			"export.download"
		],
		"time.timer-prompt": [
			"settings.timer-prompt"
		],
		"agents.work-log": [
			"task-form.agent-work",
			"task-form.agent-totals"
		],
		"agents.phone-alerts": [
			"agent-notify.connect",
			"agent-notify.command",
			"agent-notify.test"
		],
		"agents.mcp-token": [
			"settings.device-token"
		],
		"agents.ask-ai": [
			"nav.ask-ai",
			"ai.input"
		],
		"agents.connect-later": [
			"connect.client-tabs",
			"connect.command",
			"connect.status",
			"connect.prompt"
		],
		"personas.create": [
			"personas.create",
			"personas.grants"
		],
		"personas.token": [
			"personas.tokens"
		],
		"personas.assign": [
			"task-form.persona-assignees"
		],
		"personas.skills": [
			"personas.skills"
		],
		"pages.first": [
			"pages.new",
			"pages.tree"
		],
		"pages.context": [
			"pages.context"
		],
		"pages.links": [
			"page.editor",
			"page.backlinks"
		],
		"pages.versions": [
			"page.versions"
		],
		"pages.people-meetings": [
			"pages.type-picker"
		],
		"pages.import": [
			"pages.import",
			"pages.export"
		],
		"pages.task-from-selection": [
			"page.selection-menu"
		],
		"graph.map": [
			"map.canvas",
			"map.filters",
			"map.node"
		],
		"graph.enable": [
			"modules.graph"
		],
		"routines.first": [
			"routines.add",
			"routines.instance"
		],
		"routines.views": [
			"routines.view-switcher"
		],
		"routines.reminders": [
			"routine.reminder",
			"routine.to-task"
		],
		"ws.modules": [
			"modules.presets",
			"modules.list",
			"modules.hide"
		],
		"ws.invite": [
			"workspace.members"
		],
		"ws.dashboard": [
			"dashboard.activity",
			"dashboard.member"
		],
		"ws.workspaces": [
			"nav.workspace-switcher"
		],
		"ws.appearance": [
			"settings.theme"
		],
		"ws.notifications": [
			"nav.notification-bell"
		],
		"ws.privacy": [
			"settings.privacy-telemetry",
			"settings.privacy-tips"
		],
		"ws.telegram": [
			"settings.telegram"
		],
		"ws.default-landing": [
			"settings.default-landing"
		],
		"desktop.quick-add": [
			"s1",
			"s2"
		],
		"desktop.capture": [
			"s1",
			"settings.desktop-shortcuts"
		],
		"desktop.menu-bar-timer": [
			"s1",
			"s2"
		],
		"desktop.windows": [
			"task-form.open-window"
		],
		"desktop.local-workspace": [
			"local.banner",
			"local.badge",
			"nav.create-account"
		],
		"desktop.backups": [
			"settings.data-backups",
			"settings.backup-now"
		],
		"desktop.local-agent": [
			"local-personas.panel",
			"local-personas.connect"
		],
		"desktop.plugins": [
			"plugins.install",
			"plugins.help"
		],
		"desktop.whats-new": [
			"whats-new.item-tour"
		],
		"desktop.move-to-cloud": [
			"settings.data-move",
			"s2"
		],
		"ios.tabs": [
			"tab-list",
			"tab-board",
			"tab-routines",
			"workspace-pill",
			"tab-more"
		],
		"ios.pomodoro": [
			"s1"
		],
		"ios.widgets": [
			"s1"
		],
		"ios.controls": [
			"s1"
		],
		"ios.watch": [
			"s1"
		],
		"ios.siri": [
			"s1"
		],
		"ios.share": [
			"s1"
		],
		"ios.notifications": [
			"s1"
		],
		"ios.alarms": [
			"s1"
		],
		"ios.alarm-phone": [
			"s1"
		],
		"ios.telegram": [
			"s1"
		],
		"ios.face-id": [
			"s1"
		],
		"ios.ask-ai": [
			"s1"
		]
	},
	"events": {
		"onboarding_shown": {},
		"onboarding_intent": {
			"intent": {
				"enum": "intent"
			}
		},
		"connect_client_selected": {
			"client": {
				"enum": "client"
			}
		},
		"connect_command_copied": {},
		"connect_stalled": {},
		"checklist_item_done": {
			"item": {
				"enum": "item"
			}
		},
		"checklist_hidden": {},
		"tour_offered": {
			"tour": {
				"enum": "tour"
			},
			"trigger": {
				"enum": "trigger"
			}
		},
		"tour_started": {
			"tour": {
				"enum": "tour"
			},
			"trigger": {
				"enum": "trigger"
			}
		},
		"tour_step_viewed": {
			"tour": {
				"enum": "tour"
			},
			"step": {
				"step_of": "tour"
			},
			"i": {
				"int": [
					0,
					30
				]
			}
		},
		"tour_step_completed": {
			"tour": {
				"enum": "tour"
			},
			"step": {
				"step_of": "tour"
			},
			"i": {
				"int": [
					0,
					30
				]
			},
			"via": {
				"enum": "via"
			}
		},
		"tour_step_skipped": {
			"tour": {
				"enum": "tour"
			},
			"step": {
				"step_of": "tour"
			},
			"i": {
				"int": [
					0,
					30
				]
			}
		},
		"tour_step_unavailable": {
			"tour": {
				"enum": "tour"
			},
			"step": {
				"step_of": "tour"
			}
		},
		"tour_paused": {
			"tour": {
				"enum": "tour"
			},
			"i": {
				"int": [
					0,
					30
				]
			}
		},
		"tour_completed": {
			"tour": {
				"enum": "tour"
			},
			"dur": {
				"enum": "dur"
			}
		},
		"tour_skipped": {
			"tour": {
				"enum": "tour"
			},
			"i": {
				"int": [
					0,
					30
				]
			}
		},
		"tips_mode_changed": {
			"mode": {
				"enum": "mode"
			}
		},
		"screen_viewed": {
			"route": {
				"enum": "route"
			}
		},
		"feature_used": {
			"feature": {
				"enum": "feature"
			}
		},
		"local_workspace_created": {},
		"backup_created": {
			"kind": {
				"enum": "kind"
			}
		},
		"migration_started": {},
		"migration_completed": {},
		"migration_failed": {
			"phase": {
				"enum": "phase"
			}
		},
		"app_opened": {}
	}
} as const;

export type Enums = typeof CATALOG.enums;
export type EventName = keyof typeof CATALOG.events;
export type TourId = Enums['tour'][number];
export type RouteName = Enums['route'][number];
export type Platform = Enums['platform'][number];
export type AccountMode = Enums['account_mode'][number];
export type Viewport = Enums['viewport'][number];

type Spec = { enum: string } | { step_of: string } | { int: readonly [number, number] };

type PropValue<S> = S extends { enum: infer E extends keyof Enums }
	? Enums[E][number]
	: S extends { step_of: string }
		? string
		: S extends { int: readonly [number, number] }
			? number
			: never;

export type EventProps = {
	[N in EventName]: { -readonly [K in keyof (typeof CATALOG.events)[N]]: PropValue<(typeof CATALOG.events)[N][K]> };
};

export type TrackArgs<N extends EventName> = keyof EventProps[N] extends never
	? [props?: undefined]
	: [props: EventProps[N]];

const matches = (spec: Spec, value: unknown, props: Record<string, unknown>): boolean => {
	if ('enum' in spec) {
		const allowed = (CATALOG.enums as Record<string, readonly string[]>)[spec.enum];
		return typeof value === 'string' && !!allowed?.includes(value);
	}
	if ('step_of' in spec) {
		const steps = (CATALOG.steps as Record<string, readonly string[]>)[String(props[spec.step_of])];
		return typeof value === 'string' && !!steps?.includes(value);
	}
	return (
		typeof value === 'number' &&
		Number.isInteger(value) &&
		value >= spec.int[0] &&
		value <= spec.int[1]
	);
};

export const isKnownEvent = (name: string): name is EventName =>
	Object.prototype.hasOwnProperty.call(CATALOG.events, name);

export const validProps = (name: EventName, props: Record<string, unknown> = {}): boolean => {
	const specs = CATALOG.events[name] as Record<string, Spec>;
	const keys = Object.keys(props);
	if (keys.length !== Object.keys(specs).length) return false;
	return keys.every((key) => key in specs && matches(specs[key], props[key], props));
};

export const isCatalogRoute = (name: unknown): name is RouteName =>
	typeof name === 'string' && (CATALOG.enums.route as readonly string[]).includes(name);
