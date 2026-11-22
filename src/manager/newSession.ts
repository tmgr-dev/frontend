import {
	roomKey,
	type ChatMessage,
	type ManagerMachine,
	type SpawnAgent,
	type SpawnRequest,
} from './types';

export interface MachineOption {
	id: string;
	label: string;
	linked: boolean;
}

export interface SessionTemplate {
	id: string;
	prompt: string;
}

const SHORT_ID = 12;

export const AGENT_LABELS: Record<SpawnAgent, string> = {
	claude: 'Claude Code',
	codex: 'Codex',
	opencode: 'opencode',
	agy: 'agy',
};

export const SESSION_TEMPLATES: SessionTemplate[] = [
	{ id: 'blank', prompt: '' },
	{
		id: 'fixBug',
		prompt:
			'Find and fix the bug described in the task. Reproduce it with a failing test first.',
	},
	{
		id: 'feature',
		prompt:
			'Implement the task end to end. Plan first, then write tests and code.',
	},
	{
		id: 'reviewPr',
		prompt:
			'Review the pull request: read the diff, flag bugs and risks, and list what to change.',
	},
	{
		id: 'releaseNotes',
		prompt: 'Draft release notes from the commits since the last release.',
	},
];

export const machineOptions = (
	ids: string[],
	machines: ManagerMachine[],
): MachineOption[] =>
	ids.map((id) => {
		const known = machines.find((m) => m.machine === id);
		if (known?.local) return { id, label: 'This Mac', linked: false };
		return {
			id,
			label: known?.label || id.slice(0, SHORT_ID),
			linked: known ? !known.local : false,
		};
	});

export const applyTemplate = (
	form: SpawnRequest,
	templateId: string,
): SpawnRequest => {
	const tpl = SESSION_TEMPLATES.find((t) => t.id === templateId);
	return tpl ? { ...form, prompt: tpl.prompt } : form;
};

export const distinctChats = (hits: ChatMessage[]): number =>
	new Set(hits.map((h) => roomKey(h.room))).size;
