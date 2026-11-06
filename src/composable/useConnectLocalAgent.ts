import {
	CONNECT_AGENTS,
	agentInfo,
	copyConfigToken,
	detectAgents,
	errorText,
	installAgent,
	removeAgent,
	type ConnectAgentId,
	type ConnectResult,
	type DetectOut,
	type RunnableAgentId,
} from '@/local/connectAgent';
import type { LocalUser, LocalWorkspace } from '@/local/types';
import { computed, reactive, ref } from 'vue';

export const INSTALL_NOTES = [
	'Install also turns on local agent access and the Personas and Agent work modules in this workspace.',
	'Your token stays in the macOS Keychain. It is not part of the command.',
	'Each agent connects to one workspace. Installing here replaces an earlier TMGR connection.',
];
export const TRANSLOCATED_TEXT =
	'Move TMGR to Applications, open it from there, then install.';
export const SAFE_MODE_TEXT =
	'Plugin safe mode is on, so agents cannot connect. Turn it off and reopen this screen.';
export const UNSUPPORTED_TEXT =
	'Installing from this screen works on macOS only. Use Copy config instead.';
export const BRIDGE_TEXT =
	'TMGR cannot work out its own program path, so agents cannot connect from here.';
export const TIMEOUT_TEXT =
	'The command did not finish. Press Install again, or Remove.';
export const CLAUDE_FAILURE_NOTE =
	'The earlier TMGR entry in Claude Code was already removed.';

const PLACEHOLDER = '<new token id>';

export const shellQuote = (arg: string): string =>
	arg === PLACEHOLDER || /^[A-Za-z0-9_@%+=:,./~-]+$/.test(arg)
		? arg
		: `'${arg.replace(/'/g, `'\\''`)}'`;

const commandLine = (argv: string[]) => argv.map(shellQuote).join(' ');

export interface AgentView {
	status: 'ok' | 'failed' | 'timeout';
	message: string;
	note?: string;
	steps: { command: string; output: string }[];
}

const viewOf = (
	result: ConnectResult,
	okMessage: string,
	failMessage: string,
): AgentView => ({
	status: result.status,
	message:
		result.status === 'ok'
			? okMessage
			: result.status === 'timeout'
				? TIMEOUT_TEXT
				: (result.error ?? failMessage),
	steps: (result.outcome?.steps ?? []).map((s) => ({
		command: commandLine(s.display),
		output: s.output,
	})),
});

export function useConnectLocalAgent(deps: {
	workspace: () => LocalWorkspace;
	user: () => LocalUser;
	onModulesChanged?: () => void | Promise<void>;
	writeClipboard?: (text: string) => Promise<void>;
}) {
	const detect = ref<DetectOut | null>(null);
	const loading = ref(true);
	const loadError = ref('');
	const selected = reactive<Record<string, boolean>>({});
	const previewed = ref(false);
	const running = ref(false);
	const results = reactive<Record<string, AgentView>>({});
	const copied = ref<ConnectAgentId | ''>('');
	const copyError = ref('');
	const busyRemove = ref<string>('');

	const load = async () => {
		loading.value = true;
		loadError.value = '';
		try {
			detect.value = await detectAgents();
		} catch (error) {
			detect.value = null;
			loadError.value = errorText(error);
		} finally {
			loading.value = false;
		}
	};

	const detected = (id: ConnectAgentId) =>
		detect.value?.agents.find((a) => a.id === id);
	const supported = computed(() => detect.value?.platformSupported === true);

	const rows = computed(() =>
		CONNECT_AGENTS.map((info) => {
			const found = !!detected(info.id)?.found;
			const selectable = info.canInstall && found && supported.value;
			return {
				info,
				found,
				selectable,
				copyOnly: !selectable,
			};
		}),
	);

	const installBlock = computed<string | null>(() => {
		const d = detect.value;
		if (!d) return null;
		if (!d.platformSupported) return UNSUPPORTED_TEXT;
		if (d.safeMode) return SAFE_MODE_TEXT;
		if (d.bridge.reason === 'translocated') return TRANSLOCATED_TEXT;
		if (!d.bridge.ok) return BRIDGE_TEXT;
		return null;
	});

	const selectedAgents = computed(() =>
		rows.value.filter((r) => r.selectable && selected[r.info.id]),
	);

	const canPreview = computed(
		() => !installBlock.value && selectedAgents.value.length > 0,
	);
	const canInstall = computed(
		() => canPreview.value && previewed.value && !running.value,
	);

	const previewLines = computed(() =>
		previewed.value
			? selectedAgents.value.map((r) => ({
					agent: r.info.id,
					title: r.info.title,
					commands: (detected(r.info.id)?.install ?? []).map(commandLine),
				}))
			: [],
	);

	const toggle = (id: ConnectAgentId, value: boolean) => {
		selected[id] = value;
		previewed.value = false;
	};

	const preview = () => {
		if (canPreview.value) previewed.value = true;
	};

	const refreshModules = async () => {
		try {
			await deps.onModulesChanged?.();
		} catch (error) {
			console.error('Failed to refresh workspace modules', error);
		}
	};

	const install = async () => {
		if (!canInstall.value) return;
		running.value = true;
		try {
			for (const row of [...selectedAgents.value]) {
				const agent = row.info.id as RunnableAgentId;
				delete results[agent];
				const result = await installAgent({
					agent,
					workspace: deps.workspace(),
					user: deps.user(),
				});
				const view = viewOf(
					result,
					`Restart ${row.info.title} so it picks up TMGR. Then ask it to list your TMGR tasks.`,
					`Could not connect ${row.info.title}.`,
				);
				if (result.status !== 'ok' && agent === 'claude-code')
					view.note = CLAUDE_FAILURE_NOTE;
				results[agent] = view;
				if (result.modulesChanged) await refreshModules();
			}
		} finally {
			running.value = false;
		}
	};

	const remove = async (agent: RunnableAgentId) => {
		if (running.value || busyRemove.value) return;
		busyRemove.value = agent;
		try {
			delete results[agent];
			const title = agentInfo(agent).title;
			results[agent] = viewOf(
				await removeAgent({ agent }),
				`Removed TMGR from ${title}.`,
				`Could not remove TMGR from ${title}.`,
			);
		} finally {
			busyRemove.value = '';
		}
	};

	const copyConfig = async (agent: ConnectAgentId) => {
		copyError.value = '';
		copied.value = '';
		try {
			const result = await copyConfigToken({
				agent,
				workspace: deps.workspace(),
				user: deps.user(),
			});
			await (deps.writeClipboard
				? deps.writeClipboard(result.text)
				: navigator.clipboard.writeText(result.text));
			copied.value = agent;
			if (result.modulesChanged) await refreshModules();
		} catch (error) {
			copyError.value = errorText(error);
		}
	};

	return {
		detect,
		loading,
		loadError,
		selected,
		previewed,
		running,
		busyRemove,
		results,
		copied,
		copyError,
		rows,
		installBlock,
		canPreview,
		canInstall,
		previewLines,
		load,
		toggle,
		preview,
		install,
		remove,
		copyConfig,
	};
}
