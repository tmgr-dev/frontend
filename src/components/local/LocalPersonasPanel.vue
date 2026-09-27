<template>
	<div class="flex flex-col gap-4 border-t border-border pt-6">
		<header class="flex flex-col gap-1">
			<h4 class="text-sm font-semibold">On this device</h4>
			<p class="text-sm text-muted-foreground">
				Personas from your account, snapshotted into this local workspace. Their
				prompt and skills stay in this app's data folder, never in the workspace
				file, so the folder stays safe to copy or move.
			</p>
			<p v-if="!workspace" class="text-sm text-amber-600 dark:text-amber-400">
				Open a local workspace to manage personas here.
			</p>
		</header>

		<section
			v-if="workspace"
			class="flex flex-col gap-3 rounded-md border border-border p-4"
		>
			<h5 class="text-sm font-semibold">Local LLM</h5>
			<label class="flex flex-col gap-1 text-sm">
				Base URL
				<Input v-model="llmForm.baseUrl" placeholder="http://localhost:11434" />
			</label>
			<label class="flex flex-col gap-1 text-sm">
				Model
				<Input v-model="llmForm.model" placeholder="llama3.1" />
			</label>
			<label class="flex flex-col gap-1 text-sm">
				API key
				<span class="text-xs text-muted-foreground">
					{{ llmStatus.hasApiKey ? 'A key is already saved; leave blank to keep it.' : 'Optional.' }}
				</span>
				<Input v-model="llmForm.apiKey" type="password" placeholder="sk-…" />
			</label>
			<p
				v-if="llmForm.baseUrl && !llmStatus.isLocal"
				class="text-sm text-amber-600 dark:text-amber-400"
			>
				This URL is not on localhost or your LAN — the persona's traffic (and any
				key you set) will leave this machine.
			</p>
			<div>
				<Button size="sm" :disabled="savingLlm" @click="saveLlmConfig">
					{{ savingLlm ? 'Saving…' : 'Save' }}
				</Button>
			</div>
		</section>

		<section
			v-if="workspace"
			class="flex flex-col gap-3 rounded-md border border-border p-4"
		>
			<div class="flex items-center justify-between">
				<h5 class="text-sm font-semibold">Personas in this workspace</h5>
				<Button size="sm" variant="outline" :disabled="syncing" @click="sync">
					{{ syncing ? 'Syncing…' : 'Sync now' }}
				</Button>
			</div>
			<p v-if="syncError" class="text-sm text-red-600 dark:text-red-400">{{ syncError }}</p>
			<p v-if="!personas.length" class="text-sm text-muted-foreground">
				No personas synced yet.
			</p>
			<div
				v-for="persona in personas"
				:key="persona.uuid"
				class="flex flex-col gap-2 rounded border border-border p-3"
			>
				<div class="flex items-center justify-between gap-4">
					<div>
						<div class="text-sm font-medium">
							{{ persona.name }}
							<span v-if="persona.archived_at" class="ml-2 text-xs text-muted-foreground">
								(archived in the cloud)
							</span>
						</div>
						<div class="text-xs text-muted-foreground">{{ persona.description }}</div>
					</div>
					<Switch
						:checked="isEnabled(persona)"
						:disabled="!!persona.archived_at"
						@update:checked="(value: boolean) => toggle(persona, value)"
					/>
				</div>
				<div v-if="isEnabled(persona)" class="flex flex-wrap gap-x-4 gap-y-1 text-xs">
					<label
						v-for="perm in PERSONA_PERMISSIONS"
						:key="perm"
						class="flex items-center gap-1"
					>
						<input
							type="checkbox"
							:checked="draftPermissions(persona).includes(perm)"
							@change="togglePermission(persona, perm)"
						/>
						{{ perm }}
					</label>
				</div>
			</div>
		</section>
	</div>
</template>

<script lang="ts">
	import { listPersonas } from '@/actions/tmgr/personas';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Switch } from '@/components/ui/switch';
	import { tauriPersonaCache } from '@/local/personaCache';
	import { PERSONA_PERMISSIONS, type PersonaPermission } from '@/local/personaGate';
	import {
		disableLocalPersona,
		enableLocalPersona,
		listLocalPersonas,
		syncPersonasSnapshot,
		type WorkspacePersonaRow,
	} from '@/local/personas';
	import { activeLocalWorkspace, localContext } from '@/local/runtime';
	import { defineComponent, onMounted, reactive, ref } from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'LocalPersonasPanel',
		components: { Button, Input, Switch },
		setup() {
			const store = useStore();
			const workspace = activeLocalWorkspace();
			const personas = ref<WorkspacePersonaRow[]>([]);
			const syncing = ref(false);
			const syncError = ref<string | null>(null);
			const drafts = reactive<Record<string, PersonaPermission[]>>({});
			const llmForm = reactive({ baseUrl: '', model: '', apiKey: '' });
			const llmStatus = reactive({ hasApiKey: false, isLocal: true });
			const savingLlm = ref(false);

			const currentUser = () => ({
				id: Number(store.state.user?.id) || 0,
				name: store.state.user?.name ?? '',
				email: store.state.user?.email ?? '',
			});

			const ctxFor = async () => {
				if (!workspace) throw new Error('No local workspace is open');
				return localContext(workspace, currentUser());
			};

			const refreshPersonas = async () => {
				if (!workspace) return;
				const ctx = await ctxFor();
				personas.value = await listLocalPersonas(ctx);
				for (const persona of personas.value) drafts[persona.uuid] = persona.permissions ?? [];
			};

			const invoke = async <T,>(command: string, args?: Record<string, unknown>): Promise<T> => {
				const core = await import('@tauri-apps/api/core');
				return core.invoke<T>(command, args);
			};

			const refreshLlmConfig = async () => {
				const config = await invoke<{ base_url: string; model: string; has_api_key: boolean; is_local: boolean }>(
					'llm_config_get',
				);
				llmForm.baseUrl = config.base_url;
				llmForm.model = config.model;
				llmStatus.hasApiKey = config.has_api_key;
				llmStatus.isLocal = config.is_local;
			};

			onMounted(async () => {
				if (!workspace) return;
				await Promise.all([refreshPersonas(), refreshLlmConfig()]);
			});

			const isEnabled = (persona: WorkspacePersonaRow) => !!persona.enabled_at && !persona.disabled_at;

			const draftPermissions = (persona: WorkspacePersonaRow) => drafts[persona.uuid] ?? [];

			const toggle = async (persona: WorkspacePersonaRow, value: boolean) => {
				const ctx = await ctxFor();
				if (value) await enableLocalPersona(ctx, persona.uuid, draftPermissions(persona));
				else await disableLocalPersona(ctx, persona.uuid);
				await refreshPersonas();
			};

			const togglePermission = async (persona: WorkspacePersonaRow, permission: PersonaPermission) => {
				const current = draftPermissions(persona);
				drafts[persona.uuid] = current.includes(permission)
					? current.filter((p) => p !== permission)
					: [...current, permission];
				if (isEnabled(persona)) {
					const ctx = await ctxFor();
					await enableLocalPersona(ctx, persona.uuid, drafts[persona.uuid]);
				}
			};

			const sync = async () => {
				syncing.value = true;
				syncError.value = null;
				try {
					const ctx = await ctxFor();
					await syncPersonasSnapshot(ctx, () => listPersonas(false, false), tauriPersonaCache);
					await refreshPersonas();
				} catch (error) {
					syncError.value = error instanceof Error ? error.message : String(error);
				} finally {
					syncing.value = false;
				}
			};

			const saveLlmConfig = async () => {
				savingLlm.value = true;
				try {
					await invoke('llm_config_set', {
						baseUrl: llmForm.baseUrl,
						model: llmForm.model,
						apiKey: llmForm.apiKey || null,
					});
					llmForm.apiKey = '';
					await refreshLlmConfig();
				} finally {
					savingLlm.value = false;
				}
			};

			return {
				workspace,
				personas,
				syncing,
				syncError,
				llmForm,
				llmStatus,
				savingLlm,
				PERSONA_PERMISSIONS,
				isEnabled,
				draftPermissions,
				toggle,
				togglePermission,
				sync,
				saveLlmConfig,
			};
		},
	});
</script>
