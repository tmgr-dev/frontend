export const INSTALL_COMMAND = 'curl -fsSL https://tmgr.dev/install.sh | sh';

export interface CliScenario {
	id: string;
	title: string;
	benefit: string;
	example: string;
	also: string[];
	note?: string;
}

export const CLI_INTRO =
	'tmgr is a small command-line program for TMGR. It lets you work with your tasks without opening the browser, and run AI coding agents such as Claude Code and Codex where you can watch them, stop them and undo what they did.';

export const CLI_SCENARIOS: CliScenario[] = [
	{
		id: 'tasks',
		title: 'Work on tasks without leaving the terminal',
		benefit:
			'One command picks up a task: it creates the git branch, moves the task to In progress and starts the timer. When you are done, one more command stops the timer, moves the task to review and adds the pull request link. No tab switching.',
		example: 'tmgr start TM-123',
		also: [
			'tmgr status',
			'tmgr board',
			'tmgr today',
			'tmgr switch TM-124',
			'tmgr done',
			'tmgr add "Fix the login redirect"',
			'tmgr task show TM-123',
			'tmgr git install-hooks',
			'tmgr project map',
			'tmgr init zsh',
		],
		note: 'git install-hooks and project map tie your commits to the task of the current branch.',
	},
	{
		id: 'runner',
		title: 'Run AI agents under supervision',
		benefit:
			'Each agent works in its own copy of your project, inside a sandbox, and its work is saved after every turn. It cannot touch your main branch, you can look at what it is doing at any moment, and you can roll its changes back.',
		example: 'tmgr run --agent claude-code --worktree --checkpoints',
		also: [
			'tmgr attach',
			'tmgr peek',
			'tmgr send',
			'tmgr halt',
			'tmgr resume',
			'tmgr checkpoints diff',
			'tmgr checkpoints restore',
			'tmgr session summary',
			'tmgr session export --page',
			'tmgr runner trust',
			'tmgr runner allow',
		],
		note: 'session export --page saves the story of a session as a page on the task. runner trust and runner allow decide which folders and workspaces agents may work in.',
	},
	{
		id: 'decisions',
		title: 'Answer every agent question in one inbox',
		benefit:
			'When an agent needs a decision, it sends you a card with the question, the options, its recommendation and the changes to look at. You answer once in the TMGR desktop app, and the agent gets your answer and carries on. Answers you already gave can be found again, so agents do not ask the same thing twice.',
		example:
			'tmgr ask "Which database?" --option SQLite --option Postgres --recommended 1 --wait',
		also: ['tmgr decisions answers', 'tmgr mcp'],
		note: 'The Decisions inbox works in the desktop app on macOS and Linux. Agents can also ask through the MCP tools ask_owner and find_answers.',
	},
	{
		id: 'phone',
		title: 'Get a message on your phone when an agent needs you',
		benefit:
			'Walk away from the screen. A push tells you when an agent is waiting for input or has finished a long job. For something urgent, an alarm rings in the TMGR iOS app and then calls your phone if you do not answer.',
		example: 'tmgr alarm "Production is down"',
		also: [
			'tmgr notify "Build finished"',
			'tmgr hook install claude',
			'tmgr hook install codex',
		],
		note: 'Needs the TMGR iOS app and a token from Settings, Agent notifications. The same tools ship as the @tmgr/notify package for any MCP client.',
	},
	{
		id: 'autopilot',
		title: 'Let agents work through your backlog',
		benefit:
			'Point autopilot at a workspace and it starts agent teams on your tasks, with limits you set: how many teams, how long, how much to spend. It stops before you hit your plan limit and can continue after the limit resets.',
		example: 'tmgr autopilot start --max-teams 2 --max-hours 4 --dry-run',
		also: [
			'tmgr autopilot status',
			'tmgr queue rules list',
			'tmgr queue approve',
			'tmgr agents login',
			'tmgr agents sync',
		],
		note: '--dry-run only plans: nothing is started. The queue commands are experimental. agents login and agents sync keep separate accounts for your agent tools.',
	},
	{
		id: 'usage',
		title: 'See what your AI tools cost and how much limit is left',
		benefit:
			'Know the cost per day, per task and per account, and how close you are to the limit, without opening a billing page. tmgr reads the usage logs of Claude Code and Codex on your computer. It never stores your prompts.',
		example: 'tmgr usage --week --by task',
		also: [
			'tmgr usage limits',
			'tmgr usage task TM-123 --tree',
			'tmgr usage statusline install',
			'tmgr time',
		],
		note: 'Reading the logs needs your consent, and you can withdraw it any time with tmgr usage consent deny.',
	},
	{
		id: 'desktop',
		title: 'Pair it with the desktop app',
		benefit:
			'The TMGR desktop app already installs tmgr for you. Runner sessions and Decisions show up in the app, local workspaces that never touch the cloud work from the command line too, and shared skills can be attached to your personas.',
		example: 'tmgr --ws local:my-notes status',
		also: ['tmgr skills list', 'tmgr skills attach'],
	},
	{
		id: 'agents',
		title: 'Give your agents the same tools',
		benefit:
			'tmgr speaks MCP, so Claude Code, Codex, Cursor and other agents can create tasks, ask you questions and send you notifications on their own.',
		example: 'tmgr mcp',
		also: ['tmgr hook install claude', 'tmgr hook install codex'],
		note: 'Step-by-step setup for each agent is in the AI agents guide.',
	},
];

export const CLI_SAFETY: string[] = [
	'Your sign-in token is kept in the system keychain, and tmgr never prints it.',
	'tmgr self-update installs only releases with a valid signature, and checks them before replacing itself.',
	'Agents run in a sandbox by default. Leaving it needs a typed confirmation.',
	'Agents are controlled only from your own computer. The TMGR server never sends them commands.',
	'tmgr setup terminal makes copying from tmgr attach work over SSH. It shows what it will change and asks first.',
];

export const CLI_EXTRA_COMMANDS = ['mcp'];
