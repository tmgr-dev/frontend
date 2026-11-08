<template>
	<div
		v-if="show && enabledPersonas.length"
		class="mb-3 flex flex-wrap items-center gap-2 text-xs"
	>
		<select
			v-model="selected"
			class="rounded border border-line bg-surface px-2 py-1 dark:bg-surface-sunken"
		>
			<option v-for="p in enabledPersonas" :key="p.uuid" :value="p.uuid">
				{{ p.name }}
			</option>
		</select>
		<button
			v-if="!asking"
			type="button"
			class="rounded-pill border border-line px-3 py-1 disabled:opacity-50"
			@click="ask"
		>
			{{ $t('taskAgent.askPersona.ask') }}
		</button>
		<template v-else-if="pendingConfirm">
			<span class="text-ink-subtle">
				<template v-if="pendingConfirm.cutOff">
					{{
						$t('taskAgent.askPersona.cutOff', { length: pendingConfirm.length })
					}}
				</template>
				<template v-else>
					{{
						$t('taskAgent.askPersona.overLimit', {
							length: pendingConfirm.length,
							limit: pendingConfirm.limit,
						})
					}}
				</template>
			</span>
			<button
				type="button"
				class="rounded-pill border border-line px-3 py-1"
				@click="settleConfirm(true)"
			>
				{{ $t('taskAgent.askPersona.publishAnyway') }}
			</button>
			<button
				type="button"
				class="rounded-pill border border-line px-3 py-1"
				@click="settleConfirm(false)"
			>
				{{ $t('taskAgent.askPersona.discard') }}
			</button>
		</template>
		<template v-else>
			<span class="text-ink-subtle">{{
				$t('taskAgent.askPersona.asking')
			}}</span>
			<button
				type="button"
				class="rounded-pill border border-line px-3 py-1"
				@click="cancelAsk"
			>
				{{ $t('common.actions.cancel') }}
			</button>
		</template>
		<div
			v-if="partialText"
			class="flex max-h-32 w-full flex-col-reverse overflow-y-auto text-ink-subtle"
		>
			<div class="whitespace-pre-wrap">{{ partialText }}</div>
		</div>
		<span v-if="error" class="text-red-600 dark:text-red-400">{{ error }}</span>
	</div>
</template>

<script lang="ts">
	import { createLocalApi } from '@/local/api';
	import { createLlmChat } from '@/local/llmClient';
	import {
		askPersonaOnTask,
		personaToolDefinition,
	} from '@/local/personaAgent';
	import { readPersonaCache } from '@/local/personaCache';
	import { LOCAL_OWNER_ID } from '@/local/personaGate';
	import {
		ASK_TOTAL_TIMEOUT_MS,
		DEFAULT_MAX_ANSWER_CHARS,
		DEFAULT_MAX_TOKENS,
		fetchPersonaLimits,
	} from '@/local/personaLimits';
	import {
		ensureDefaultLocalPersona,
		listLocalPersonas,
		type WorkspacePersonaRow,
	} from '@/local/personas';
	import { activeLocalWorkspace, localContext } from '@/local/runtime';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
	} from 'vue';

	export default defineComponent({
		name: 'AskPersonaButton',
		props: { taskId: { type: Number, required: true } },
		emits: ['posted'],
		setup(props, { emit }) {
			const workspace = activeLocalWorkspace();
			const show = isDesktopApp() && !!workspace;
			const router = createLocalApi();

			const personas = ref<WorkspacePersonaRow[]>([]);
			const selected = ref<string | null>(null);
			const asking = ref(false);
			const error = ref<string | null>(null);
			const partialText = ref('');
			let cancelCurrent: (() => Promise<void>) | null = null;
			const pendingConfirm = ref<{
				length: number;
				limit: number;
				cutOff: boolean;
			} | null>(null);
			let settleConfirmation: ((publish: boolean) => void) | null = null;
			let cancelRequested = false;

			const settleConfirm = (publish: boolean) => {
				const settle = settleConfirmation;
				settleConfirmation = null;
				pendingConfirm.value = null;
				settle?.(publish);
			};

			const confirmPublish = (info: {
				length: number;
				limit: number;
				cutOff: boolean;
			}) =>
				new Promise<boolean>((resolve) => {
					settleConfirmation = resolve;
					pendingConfirm.value = info;
				});

			// Asking needs agent_work:write to record the run and comments:write to post the answer.
			const enabledPersonas = computed(() =>
				personas.value.filter(
					(p) =>
						!p.disabled_at &&
						!p.archived_at &&
						p.permissions?.includes('agent_work:write') &&
						p.permissions?.includes('comments:write'),
				),
			);

			const currentUser = () =>
				store.getters?.isLocalOnly
					? {
							id: LOCAL_OWNER_ID,
							name:
								store.state.user?.name || store.state.localProfile?.name || '',
							email: '',
						}
					: {
							id: Number(store.state.user?.id) || 0,
							name: store.state.user?.name ?? '',
							email: store.state.user?.email ?? '',
						};

			onMounted(async () => {
				if (!show || !workspace) return;
				const ctx = await localContext(workspace, currentUser());
				if (store.getters?.isLocalOnly) {
					try {
						await ensureDefaultLocalPersona(ctx);
					} catch {}
				}
				personas.value = await listLocalPersonas(ctx);
				selected.value = enabledPersonas.value[0]?.uuid ?? null;
			});

			const ask = async () => {
				if (!workspace || !selected.value) return;
				const persona = enabledPersonas.value.find(
					(p) => p.uuid === selected.value,
				);
				if (!persona) return;
				asking.value = true;
				error.value = null;
				partialText.value = '';
				cancelRequested = false;
				let wrote = false;
				let posted = false;
				try {
					const ctx = await localContext(workspace, currentUser());
					const cached = await readPersonaCache(persona.uuid, currentUser().id);
					const systemPrompt =
						cached?.system_prompt ||
						`You are ${persona.name}. ${persona.description ?? ''}`;
					const grantedPermissions = persona.permissions ?? [];
					const limits = store.getters?.isLocalOnly
						? {
								maxTokens: DEFAULT_MAX_TOKENS,
								maxAnswerChars: DEFAULT_MAX_ANSWER_CHARS,
							}
						: await fetchPersonaLimits();
					const { chat, cancel } = createLlmChat(
						[personaToolDefinition(grantedPermissions)],
						currentUser().id,
						{
							maxTokens: limits.maxTokens,
							deadlineAt: Date.now() + ASK_TOTAL_TIMEOUT_MS,
						},
					);
					cancelCurrent = cancel;
					if (cancelRequested) return;
					const result = await askPersonaOnTask({
						ctx,
						router,
						persona: { uuid: persona.uuid, name: persona.name },
						taskId: props.taskId,
						systemPrompt,
						grantedPermissions,
						chat,
						maxAnswerChars: limits.maxAnswerChars,
						confirmPublish,
						onText: (text) => {
							partialText.value = text;
						},
						isCancelled: () => cancelRequested,
						onToolCall: (method: string) => {
							if (
								typeof method === 'string' &&
								String(method).toUpperCase() !== 'GET'
							)
								wrote = true;
						},
					});
					posted = !!result.posted;
				} catch (e) {
					if (!cancelRequested)
						error.value = e instanceof Error ? e.message : String(e);
				} finally {
					if (posted || wrote) emit('posted');
					asking.value = false;
					if (cancelRequested || !error.value) partialText.value = '';
					cancelCurrent = null;
				}
			};

			const cancelAsk = () => {
				cancelRequested = true;
				settleConfirm(false);
				return cancelCurrent?.();
			};

			onBeforeUnmount(() => {
				Promise.resolve(cancelAsk()).catch(() => {});
			});

			return {
				show,
				enabledPersonas,
				selected,
				asking,
				error,
				partialText,
				pendingConfirm,
				settleConfirm,
				ask,
				cancelAsk,
			};
		},
	});
</script>
