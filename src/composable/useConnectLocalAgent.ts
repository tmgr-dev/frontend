import { t } from '@/i18n';
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

export const installNotes = (): string[] => [
	t('desktop.helpers.connectAgent.notes.modules'),
	t('desktop.helpers.connectAgent.notes.keychain'),
	t('desktop.helpers.connectAgent.notes.oneWorkspace'),
];
export const translocatedText = (): string =>
	t('desktop.helpers.connectAgent.translocated');
export const safeModeText = (): string =>
	t('desktop.helpers.connectAgent.safeMode');
export const unsupportedText = (): string =>
	t('desktop.helpers.connectAgent.unsupported');
export const bridgeText = (): string =>
	t('desktop.helpers.connectAgent.bridge');
export const timeoutText = (): string =>
	t('desktop.helpers.connectAgent.timeout');
export const claudeFailureNote = (): string =>
	t('desktop.helpers.connectAgent.claudeFailureNote');

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
				? timeoutText()
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

	const installBlock = computed<string | null>(() => {
		const d = detect.value;
		if (!d) return null;
		if (!d.platformSupported) return unsupportedText();
		if (d.safeMode) return safeModeText();
		if (d.bridge.reason === 'translocated') return translocatedText();
		if (!d.bridge.ok) return bridgeText();
		return null;
	});

	const rows = computed(() =>
		CONNECT_AGENTS.map((info) => {
			const found = !!detected(info.id)?.found;
			const selectable =
				info.canInstall && found && supported.value && !installBlock.value;
			return {
				info,
				found,
				selectable,
				copyOnly: !selectable || !!installBlock.value,
			};
		}),
	);

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
					t('desktop.helpers.connectAgent.installed', {
						title: row.info.title,
					}),
					t('desktop.helpers.connectAgent.installFailed', {
						title: row.info.title,
					}),
				);
				if (
					result.status !== 'ok' &&
					agent === 'claude-code' &&
					(result.outcome?.steps.length ?? 0) > 0
				)
					view.note = claudeFailureNote();
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
				t('desktop.helpers.connectAgent.removed', { title }),
				t('desktop.helpers.connectAgent.removeFailed', { title }),
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
