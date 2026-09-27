<template>
	<div v-if="show && enabledPersonas.length" class="mb-3 flex items-center gap-2 text-xs">
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
			Ask persona
		</button>
		<template v-else>
			<span class="text-ink-subtle">Asking…</span>
			<button
				type="button"
				class="rounded-pill border border-line px-3 py-1"
				@click="cancelAsk"
			>
				Cancel
			</button>
		</template>
		<span v-if="error" class="text-red-600 dark:text-red-400">{{ error }}</span>
	</div>
</template>

<script lang="ts">
	import { createLocalApi } from '@/local/api';
	import { createLlmChat } from '@/local/llmClient';
	import { readPersonaCache } from '@/local/personaCache';
	import { askPersonaOnTask, personaToolDefinition } from '@/local/personaAgent';
	import { listLocalPersonas, type WorkspacePersonaRow } from '@/local/personas';
	import { activeLocalWorkspace, localContext } from '@/local/runtime';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import { computed, defineComponent, onMounted, ref } from 'vue';

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
			let cancelCurrent: (() => Promise<void>) | null = null;

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

			const currentUser = () => ({
				id: Number(store.state.user?.id) || 0,
				name: store.state.user?.name ?? '',
				email: store.state.user?.email ?? '',
			});

			onMounted(async () => {
				if (!show || !workspace) return;
				const ctx = await localContext(workspace, currentUser());
				personas.value = await listLocalPersonas(ctx);
				selected.value = enabledPersonas.value[0]?.uuid ?? null;
			});

			const ask = async () => {
				if (!workspace || !selected.value) return;
				const persona = enabledPersonas.value.find((p) => p.uuid === selected.value);
				if (!persona) return;
				asking.value = true;
				error.value = null;
				try {
					const ctx = await localContext(workspace, currentUser());
					const cached = await readPersonaCache(persona.uuid);
					const systemPrompt = cached?.system_prompt || `You are ${persona.name}. ${persona.description ?? ''}`;
					const grantedPermissions = persona.permissions ?? [];
					const { chat, cancel } = createLlmChat([personaToolDefinition(grantedPermissions)]);
					cancelCurrent = cancel;
					await askPersonaOnTask({
						ctx,
						router,
						persona: { uuid: persona.uuid, name: persona.name },
						taskId: props.taskId,
						systemPrompt,
						grantedPermissions,
						chat,
					});
					emit('posted');
				} catch (e) {
					error.value = e instanceof Error ? e.message : String(e);
				} finally {
					asking.value = false;
					cancelCurrent = null;
				}
			};

			const cancelAsk = () => cancelCurrent?.();

			return { show, enabledPersonas, selected, asking, error, ask, cancelAsk };
		},
	});
</script>
