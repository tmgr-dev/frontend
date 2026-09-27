<script setup lang="ts">
	import { createLocalApi } from '@/local/api';
	import { createLlmChat } from '@/local/llmClient';
	import { readPersonaCache } from '@/local/personaCache';
	import { askPersonaOnTask, personaToolDefinition } from '@/local/personaAgent';
	import { listLocalPersonas, type WorkspacePersonaRow } from '@/local/personas';
	import { activeLocalWorkspace, localContext } from '@/local/runtime';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import { computed, onMounted, ref } from 'vue';

	const props = defineProps<{ taskId: number }>();
	const emit = defineEmits<{ (e: 'posted'): void }>();

	const workspace = activeLocalWorkspace();
	const show = isDesktopApp() && !!workspace;
	const router = createLocalApi();

	const personas = ref<WorkspacePersonaRow[]>([]);
	const selected = ref<string | null>(null);
	const asking = ref(false);
	const error = ref<string | null>(null);

	const enabledPersonas = computed(() =>
		personas.value.filter((p) => p.permissions && !p.disabled_at && !p.archived_at),
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
			const { chat } = createLlmChat([personaToolDefinition(grantedPermissions)]);
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
		}
	};
</script>

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
			type="button"
			class="rounded-pill border border-line px-3 py-1 disabled:opacity-50"
			:disabled="asking"
			@click="ask"
		>
			{{ asking ? 'Asking…' : 'Ask persona' }}
		</button>
		<span v-if="error" class="text-red-600 dark:text-red-400">{{ error }}</span>
	</div>
</template>
