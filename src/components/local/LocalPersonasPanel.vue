<template>
	<div id="local-personas-panel" class="flex flex-col gap-4 border-t border-border pt-6">
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

		<div
			v-if="pendingConnect"
			class="flex items-start justify-between gap-4 rounded-md border border-amber-300 bg-amber-50 p-3 text-sm dark:border-amber-700 dark:bg-amber-950/40"
		>
			<p class="text-amber-800 dark:text-amber-200">
				The plugin <strong>{{ pendingConnect.pluginName }}</strong> asks to connect
				its companion. Choose a persona below and press “Connect an agent”.
			</p>
			<Button size="sm" variant="outline" @click="cancelPendingConnect">Decline</Button>
		</div>

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
				v-if="llmForm.baseUrl && !isTypedUrlLocal"
				class="text-sm text-amber-600 dark:text-amber-400"
			>
				This URL is not on localhost or your LAN — the persona's traffic (and any
				key you set) will leave this machine.
			</p>
			<div class="flex gap-2">
				<Button size="sm" :disabled="savingLlm" @click="saveLlmConfig">
					{{ savingLlm ? 'Saving…' : 'Save' }}
				</Button>
				<Button v-if="llmStatus.hasApiKey" size="sm" variant="outline" :disabled="savingLlm" @click="removeApiKey">
					Remove key
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

				<div v-if="canConnect(persona)" class="flex flex-col gap-2 border-t border-border pt-2">
					<div class="flex items-center justify-between">
						<Button size="sm" variant="outline" @click="openConnect(persona)">
							Connect an agent
						</Button>
						<Button
							v-if="tokensFor(persona).some((t) => !t.revokedAt)"
							size="sm"
							variant="outline"
							@click="revokeAllForPersona(persona)"
						>
							Revoke all
						</Button>
					</div>
					<p v-if="!tokensFor(persona).length" class="text-xs text-muted-foreground">
						No agent connections yet.
					</p>
					<div
						v-for="token in tokensFor(persona)"
						:key="token.id"
						class="flex items-center justify-between gap-3 rounded border border-border p-2"
					>
						<div class="min-w-0">
							<p class="flex items-center gap-2 truncate text-xs font-medium">
								{{ token.label }}
								<span
									:class="[
										'rounded px-1.5 py-0.5 text-2xs font-semibold uppercase',
										token.revokedAt
											? 'bg-muted text-muted-foreground'
											: 'bg-emerald-100 text-emerald-800 dark:bg-emerald-950 dark:text-emerald-300',
									]"
								>
									{{ token.revokedAt ? 'revoked' : 'active' }}
								</span>
								<span v-if="token.pluginId" class="text-muted-foreground">
									via plugin {{ token.pluginId }}
								</span>
							</p>
							<p class="truncate text-2xs text-muted-foreground">
								{{ token.prefix }}… · created {{ formatDate(token.createdAt) }} · expires
								{{ formatDate(token.expiresAt) }} · last used
								{{ token.lastUsedAt ? formatDate(token.lastUsedAt) : 'never' }}
							</p>
						</div>
						<Button
							v-if="!token.revokedAt"
							variant="outline"
							size="sm"
							class="shrink-0"
							@click="revokeToken(token)"
						>
							Revoke
						</Button>
					</div>
				</div>
			</div>
		</section>

		<section
			v-if="workspace"
			class="flex flex-col gap-3 rounded-md border border-border p-4"
		>
			<div class="flex items-center justify-between">
				<h5 class="text-sm font-semibold">Local agent access</h5>
				<Switch
					:checked="accessStatus.enabled"
					:disabled="accessToggling"
					@update:checked="toggleAccess"
				/>
			</div>
			<p class="text-xs text-muted-foreground">{{ accessStatusLine }}</p>
		</section>

		<LocalPersonaConnectDialog
			v-if="connectPersona && workspace"
			:open="!!connectPersona"
			:persona="connectPersona"
			:workspace="workspace"
			:initial-label="pendingConnect?.label ?? ''"
			:plugin-id="pendingConnect?.pluginId ?? null"
			@update:open="(value: boolean) => { if (!value) connectPersona = null; }"
			@issued="onIssued"
		/>
	</div>
</template>

<script lang="ts">
	import { listPersonas } from '@/actions/tmgr/personas';
	import LocalPersonaConnectDialog from '@/components/local/LocalPersonaConnectDialog.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Switch } from '@/components/ui/switch';
	import {
		getLocalAccessStatus,
		listLocalTokens,
		revokeAllLocalTokens,
		revokeLocalToken,
		setLocalAccessEnabled,
		type LocalAccessStatus,
		type TokenInfo,
	} from '@/local/localTokens';
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
	import { localAccessConnect, resolveLocalAccessConnect } from '@/pluginSystem/state';
	import { computed, defineComponent, onMounted, reactive, ref } from 'vue';
	import { useStore } from 'vuex';

	/** Mirrors the Rust classify_url check, so the warning reacts to what's typed, not last saved. */
	const isLocalLlmUrl = (input: string): boolean => {
		try {
			const { hostname } = new URL(input);
			const host = hostname.replace(/^\[|\]$/g, '');
			if (host === 'localhost' || host === '::1' || host.endsWith('.local')) return true;
			const v4 = host.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.\d{1,3}$/);
			if (v4) {
				const [a, b] = [Number(v4[1]), Number(v4[2])];
				return a === 127 || a === 10 || (a === 172 && b >= 16 && b <= 31) || (a === 192 && b === 168) || (a === 169 && b === 254);
			}
			return false;
		} catch {
			return false;
		}
	};

	export default defineComponent({
		name: 'LocalPersonasPanel',
		components: { Button, Input, LocalPersonaConnectDialog, Switch },
		setup() {
			const store = useStore();
			const workspace = activeLocalWorkspace();
			const personas = ref<WorkspacePersonaRow[]>([]);
			const syncing = ref(false);
			const syncError = ref<string | null>(null);
			const drafts = reactive<Record<string, PersonaPermission[]>>({});
			const llmForm = reactive({ baseUrl: '', model: '', apiKey: '' });
			const isTypedUrlLocal = computed(() => isLocalLlmUrl(llmForm.baseUrl));
			const llmStatus = reactive({ hasApiKey: false });
			const savingLlm = ref(false);
			const tokens = ref<TokenInfo[]>([]);
			const connectPersona = ref<WorkspacePersonaRow | null>(null);
			const accessStatus = reactive<LocalAccessStatus>({
				enabled: false,
				listening: false,
				socketPath: null,
				safeMode: false,
				ready: false,
				bridgeCommand: '',
			});
			const accessToggling = ref(false);

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
			};

			const refreshTokens = async () => {
				if (!workspace) return;
				tokens.value = await listLocalTokens(workspace.code);
			};

			const refreshAccessStatus = async () => {
				Object.assign(accessStatus, await getLocalAccessStatus());
			};

			onMounted(async () => {
				if (!workspace) return;
				await Promise.all([refreshPersonas(), refreshLlmConfig(), refreshTokens(), refreshAccessStatus()]);
			});

			const isEnabled = (persona: WorkspacePersonaRow) => !!persona.enabled_at && !persona.disabled_at;

			const canConnect = (persona: WorkspacePersonaRow) => isEnabled(persona) && !persona.archived_at;

			const tokensFor = (persona: WorkspacePersonaRow) =>
				tokens.value.filter((t) => t.personaUuid === persona.uuid);

			const openConnect = (persona: WorkspacePersonaRow) => {
				connectPersona.value = persona;
			};

			const pendingConnect = computed(() => localAccessConnect.current);

			const cancelPendingConnect = () => resolveLocalAccessConnect({ status: 'cancelled' });

			const onIssued = async (token: TokenInfo) => {
				if (pendingConnect.value && token.pluginId === pendingConnect.value.pluginId) {
					resolveLocalAccessConnect({ status: 'connected', tokenId: token.id, prefix: token.prefix });
				}
				await refreshTokens();
			};

			const formatDate = (date: string) => new Date(date).toLocaleDateString();

			const revokeToken = async (token: TokenInfo) => {
				if (!window.confirm(`Revoke the "${token.label}" connection?`)) return;
				await revokeLocalToken(token.id);
				await refreshTokens();
			};

			const revokeAllForPersona = async (persona: WorkspacePersonaRow) => {
				if (!workspace) return;
				if (!window.confirm('Revoke every agent connection for this persona?')) return;
				await revokeAllLocalTokens({ personaUuid: persona.uuid, workspaceCode: workspace.code });
				await refreshTokens();
			};

			const accessStatusLine = computed(() => {
				if (accessStatus.safeMode) return 'safe mode — closed';
				if (!accessStatus.enabled) return 'off';
				if (accessStatus.listening && accessStatus.socketPath) return `listening at ${accessStatus.socketPath}`;
				return 'off';
			});

			const toggleAccess = async (value: boolean) => {
				accessToggling.value = true;
				try {
					await setLocalAccessEnabled(value);
					await refreshAccessStatus();
				} finally {
					accessToggling.value = false;
				}
			};

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
					await syncPersonasSnapshot(ctx, () => listPersonas(true, false), tauriPersonaCache);
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
						clearApiKey: false,
					});
					llmForm.apiKey = '';
					await refreshLlmConfig();
				} finally {
					savingLlm.value = false;
				}
			};

			const removeApiKey = async () => {
				savingLlm.value = true;
				try {
					await invoke('llm_config_set', {
						baseUrl: llmForm.baseUrl,
						model: llmForm.model,
						apiKey: null,
						clearApiKey: true,
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
				isTypedUrlLocal,
				savingLlm,
				PERSONA_PERMISSIONS,
				isEnabled,
				draftPermissions,
				toggle,
				togglePermission,
				sync,
				saveLlmConfig,
				removeApiKey,
				tokens,
				connectPersona,
				accessStatus,
				accessToggling,
				accessStatusLine,
				canConnect,
				tokensFor,
				openConnect,
				onIssued,
				pendingConnect,
				cancelPendingConnect,
				formatDate,
				revokeToken,
				revokeAllForPersona,
				toggleAccess,
			};
		},
	});
</script>
