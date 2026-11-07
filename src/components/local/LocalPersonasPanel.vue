<template>
	<div
		id="local-personas-panel"
		data-tour="local-personas.panel"
		class="flex flex-col gap-4 border-t border-border pt-6"
	>
		<header class="flex flex-col gap-1">
			<h4 class="text-sm font-semibold">
				{{ localOnly ? 'Agents on this device' : 'On this device' }}
			</h4>
			<p v-if="localOnly" class="text-sm text-muted-foreground">
				Connect Claude Code or any MCP client to this workspace without an
				account. Each agent is a local persona: press “Connect an agent”, copy
				the command into your terminal, and the agent works through its own
				permissions.
			</p>
			<p v-else class="text-sm text-muted-foreground">
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
				The plugin <strong>{{ pendingConnect.pluginName }}</strong> asks to
				connect its companion. Choose a persona below and press “Connect an
				agent”.
			</p>
			<Button size="sm" variant="outline" @click="cancelPendingConnect"
				>Decline</Button
			>
		</div>

		<LocalAiSettings v-if="workspace" />

		<section
			v-if="workspace"
			class="flex flex-col gap-3 rounded-md border border-border p-4"
		>
			<div class="flex items-center justify-between">
				<h5 class="text-sm font-semibold">Personas in this workspace</h5>
				<Button
					v-if="!localOnly"
					size="sm"
					variant="outline"
					:disabled="syncing"
					@click="sync"
				>
					{{ syncing ? 'Syncing…' : 'Sync now' }}
				</Button>
				<Button
					v-else
					size="sm"
					variant="outline"
					:data-tour="personas.length ? undefined : 'local-personas.connect'"
					@click="showAddForm = !showAddForm"
				>
					Add local agent
				</Button>
			</div>
			<form
				v-if="localOnly && showAddForm"
				class="flex flex-col gap-2 rounded border border-border p-3"
				@submit.prevent="addLocalAgent"
			>
				<label class="flex flex-col gap-1 text-sm">
					Name
					<Input
						v-model="addForm.name"
						maxlength="60"
						placeholder="Claude Code"
					/>
				</label>
				<label class="flex flex-col gap-1 text-sm">
					Description (optional)
					<Input
						v-model="addForm.description"
						placeholder="What this agent is for"
					/>
				</label>
				<p v-if="addError" class="text-sm text-red-600 dark:text-red-400">
					{{ addError }}
				</p>
				<div class="flex gap-2">
					<Button
						type="submit"
						size="sm"
						:disabled="adding || !addForm.name.trim()"
					>
						{{ adding ? 'Adding…' : 'Add agent' }}
					</Button>
					<Button
						type="button"
						size="sm"
						variant="outline"
						@click="showAddForm = false"
					>
						Cancel
					</Button>
				</div>
			</form>
			<p v-if="syncError" class="text-sm text-red-600 dark:text-red-400">
				{{ syncError }}
			</p>
			<p v-if="!personas.length" class="text-sm text-muted-foreground">
				{{ localOnly ? 'No local agents yet.' : 'No personas synced yet.' }}
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
							<span
								v-if="persona.archived_at && !localOnly"
								class="ml-2 text-xs text-muted-foreground"
							>
								(archived in the cloud)
							</span>
						</div>
						<div class="text-xs text-muted-foreground">
							{{ persona.description }}
						</div>
					</div>
					<Switch
						:checked="isEnabled(persona)"
						:disabled="!!persona.archived_at"
						@update:checked="(value: boolean) => toggle(persona, value)"
					/>
				</div>
				<div
					v-if="isEnabled(persona)"
					class="flex flex-wrap gap-x-4 gap-y-1 text-xs"
				>
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

				<div
					v-if="
						skillLibrary &&
						(skillLibrary.skills.length || attachedSkills(persona).length)
					"
					class="flex flex-col gap-2 border-t border-border pt-2"
				>
					<p class="text-xs font-medium">Library skills</p>
					<div
						v-for="skill in attachedSkills(persona)"
						:key="skill.slug"
						class="flex items-center justify-between gap-3 text-xs"
					>
						<span class="truncate">{{ skill.title }} ({{ skill.slug }})</span>
						<Button
							size="sm"
							variant="outline"
							@click="detachSkill(persona, skill.slug)"
						>
							Detach
						</Button>
					</div>
					<p v-if="skillError" class="text-xs text-red-600 dark:text-red-400">
						{{ skillError }}
					</p>
					<select
						v-if="unattachedSkills(persona).length"
						class="w-fit rounded border border-border bg-background px-2 py-1 text-xs"
						:disabled="attachedSkills(persona).length >= attachedLimit"
						@change="
							(e) => {
								attachSkill(persona, (e.target as HTMLSelectElement).value);
								(e.target as HTMLSelectElement).value = '';
							}
						"
					>
						<option value="">
							{{
								attachedSkills(persona).length >= attachedLimit
									? `Limit of ${attachedLimit} skills reached`
									: 'Attach a library skill…'
							}}
						</option>
						<option
							v-for="skill in unattachedSkills(persona)"
							:key="skill.slug"
							:value="skill.slug"
						>
							{{ skill.title }} ({{ skill.slug }})
						</option>
					</select>
				</div>

				<div
					v-if="canConnect(persona) && localAccessAvailable"
					class="flex flex-col gap-2 border-t border-border pt-2"
				>
					<div class="flex items-center justify-between">
						<Button
							v-if="!isDemo"
							size="sm"
							variant="outline"
							data-tour="local-personas.connect"
							@click="openConnect(persona)"
						>
							Connect an agent
						</Button>
						<span v-else class="text-xs text-ink-subtle">
							Not available in the demo
						</span>
						<Button
							v-if="tokensFor(persona).some((t) => !t.revokedAt)"
							size="sm"
							variant="outline"
							@click="revokeAllForPersona(persona)"
						>
							Revoke all
						</Button>
					</div>
					<p
						v-if="!tokensFor(persona).length"
						class="text-xs text-muted-foreground"
					>
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
								{{ token.prefix }}… · created
								{{ formatDate(token.createdAt) }} · expires
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
					v-if="localAccessAvailable"
					data-tour="local-personas.access"
					:checked="accessStatus.enabled"
					:disabled="accessToggling"
					@update:checked="toggleAccess"
				/>
			</div>
			<p class="text-xs text-muted-foreground">
				{{
					localAccessAvailable
						? accessStatusLine
						: 'Local access is not available on Windows yet.'
				}}
			</p>
			<RouterLink
				v-if="localAccessAvailable && !isDemo"
				class="self-start text-sm font-medium text-brand underline hover:opacity-80"
				:to="{
					name: 'ConnectLocalAgent',
					params: { workspace_code: workspace.code },
				}"
			>
				Connect your agent
			</RouterLink>
		</section>

		<LocalPersonaConnectDialog
			v-if="connectPersona && workspace"
			:open="!!connectPersona"
			:persona="connectPersona"
			:workspace="workspace"
			:initial-label="pendingConnect?.label ?? ''"
			:plugin-id="pendingConnect?.pluginId ?? null"
			@update:open="
				(value: boolean) => {
					if (!value) connectPersona = null;
				}
			"
			@issued="onIssued"
		/>
	</div>
</template>

<script lang="ts">
	import { listPersonas } from '@/actions/tmgr/personas';
	import LocalAiSettings from '@/components/local/LocalAiSettings.vue';
	import LocalPersonaConnectDialog from '@/components/local/LocalPersonaConnectDialog.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Switch } from '@/components/ui/switch';
	import { useDemoWorkspace } from '@/composable/useDemoWorkspace';
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
	import {
		LOCAL_OWNER_ID,
		PERSONA_PERMISSIONS,
		type PersonaPermission,
	} from '@/local/personaGate';
	import {
		createLocalPersona,
		disableLocalPersona,
		enableLocalPersona,
		ensureDefaultLocalPersona,
		listLocalPersonas,
		syncPersonasSnapshot,
		type WorkspacePersonaRow,
	} from '@/local/personas';
	import { activeLocalWorkspace, localContext } from '@/local/runtime';
	import {
		attach,
		detach,
		type LibraryFile,
		type LibrarySkill,
	} from '@/local/skillLibrary';
	import { skillLibraryRevision } from '@/local/skillLibraryRevision';
	import {
		readSkillLibrary,
		updateSkillLibrary,
	} from '@/local/tauriSkillLibrary';
	import {
		localAccessConnect,
		resolveLocalAccessConnect,
	} from '@/pluginSystem/state';
	import { supportsLocalAccess } from '@/utils/desktop';
	import { ATTACHED_SKILL_LIMIT } from '@/utils/personas';
	import {
		computed,
		defineComponent,
		onMounted,
		reactive,
		ref,
		watch,
	} from 'vue';
	import { RouterLink } from 'vue-router';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'LocalPersonasPanel',
		components: {
			Button,
			Input,
			LocalAiSettings,
			LocalPersonaConnectDialog,
			RouterLink,
			Switch,
		},
		setup() {
			const store = useStore();
			const workspace = activeLocalWorkspace();
			const personas = ref<WorkspacePersonaRow[]>([]);
			const syncing = ref(false);
			const syncError = ref<string | null>(null);
			const drafts = reactive<Record<string, PersonaPermission[]>>({});
			const tokens = ref<TokenInfo[]>([]);
			const connectPersona = ref<WorkspacePersonaRow | null>(null);
			const { isDemo } = useDemoWorkspace();
			const accessStatus = reactive<LocalAccessStatus>({
				enabled: false,
				listening: false,
				socketPath: null,
				safeMode: false,
				ready: false,
				bridgeCommand: '',
			});
			const accessToggling = ref(false);
			const localAccessAvailable = supportsLocalAccess();

			const localOnly = !!store.getters?.isLocalOnly;
			const showAddForm = ref(false);
			const adding = ref(false);
			const addError = ref<string | null>(null);
			const addForm = reactive({ name: '', description: '' });

			const currentUser = () =>
				localOnly
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

			const ctxFor = async () => {
				if (!workspace) throw new Error('No local workspace is open');
				return localContext(workspace, currentUser());
			};

			const addLocalAgent = async () => {
				adding.value = true;
				addError.value = null;
				try {
					await createLocalPersona(await ctxFor(), {
						name: addForm.name,
						description: addForm.description,
					});
					addForm.name = '';
					addForm.description = '';
					showAddForm.value = false;
					await refreshPersonas();
				} catch (error) {
					addError.value =
						error instanceof Error ? error.message : String(error);
				} finally {
					adding.value = false;
				}
			};

			const skillLibrary = ref<LibraryFile | null>(null);
			const skillError = ref<string | null>(null);

			const refreshSkillLibrary = async () => {
				const id = currentUser().id;
				if (!id) return;
				try {
					skillLibrary.value = await readSkillLibrary(id);
				} catch {
					skillLibrary.value = null;
				}
			};

			watch(skillLibraryRevision, refreshSkillLibrary);
			watch(() => currentUser().id, refreshSkillLibrary);

			const attachedSkills = (persona: WorkspacePersonaRow): LibrarySkill[] =>
				(skillLibrary.value?.attachments ?? [])
					.filter((a) => a.persona_uuid === persona.uuid)
					.flatMap((a) => {
						const skill = skillLibrary.value?.skills.find(
							(item) => item.slug === a.slug,
						);
						return skill ? [skill] : [];
					});

			const unattachedSkills = (persona: WorkspacePersonaRow) => {
				const taken = new Set(attachedSkills(persona).map((s) => s.slug));
				return (skillLibrary.value?.skills ?? []).filter(
					(s) => !taken.has(s.slug),
				);
			};

			const changeAttachment = async (
				change: (lib: LibraryFile) => LibraryFile,
			) => {
				skillError.value = null;
				try {
					await updateSkillLibrary(currentUser().id, (lib) => ({
						library: change(lib),
						result: null,
					}));
				} catch (error) {
					skillError.value =
						error instanceof Error ? error.message : String(error);
				}
				await refreshSkillLibrary();
			};

			const attachSkill = (persona: WorkspacePersonaRow, slug: string) =>
				slug
					? changeAttachment((lib) => attach(lib, persona.uuid, slug))
					: undefined;
			const detachSkill = (persona: WorkspacePersonaRow, slug: string) =>
				changeAttachment((lib) => detach(lib, persona.uuid, slug));

			const refreshPersonas = async () => {
				if (!workspace) return;
				refreshSkillLibrary();
				const ctx = await ctxFor();
				personas.value = await listLocalPersonas(ctx);
				for (const persona of personas.value)
					drafts[persona.uuid] = persona.permissions ?? [];
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
				if (localOnly) {
					try {
						await ensureDefaultLocalPersona(await ctxFor());
					} catch (error) {
						console.error(
							'[personas] failed to ensure the default agent',
							error,
						);
					}
				}
				await Promise.all([
					refreshPersonas(),
					refreshTokens(),
					localAccessAvailable && refreshAccessStatus(),
				]);
			});

			const isEnabled = (persona: WorkspacePersonaRow) =>
				!!persona.enabled_at && !persona.disabled_at;

			const canConnect = (persona: WorkspacePersonaRow) =>
				isEnabled(persona) && !persona.archived_at;

			const tokensFor = (persona: WorkspacePersonaRow) =>
				tokens.value.filter((t) => t.personaUuid === persona.uuid);

			const openConnect = (persona: WorkspacePersonaRow) => {
				connectPersona.value = persona;
			};

			const pendingConnect = computed(() => localAccessConnect.current);

			const cancelPendingConnect = () =>
				resolveLocalAccessConnect({ status: 'cancelled' });

			const onIssued = async (token: TokenInfo) => {
				if (
					pendingConnect.value &&
					token.pluginId === pendingConnect.value.pluginId
				) {
					resolveLocalAccessConnect({
						status: 'connected',
						tokenId: token.id,
						prefix: token.prefix,
					});
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
				if (!window.confirm('Revoke every agent connection for this persona?'))
					return;
				await revokeAllLocalTokens({
					personaUuid: persona.uuid,
					workspaceCode: workspace.code,
				});
				await refreshTokens();
			};

			const accessStatusLine = computed(() => {
				if (accessStatus.safeMode) return 'safe mode — closed';
				if (!accessStatus.enabled) return 'off';
				if (accessStatus.listening && accessStatus.socketPath)
					return `listening at ${accessStatus.socketPath}`;
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

			const draftPermissions = (persona: WorkspacePersonaRow) =>
				drafts[persona.uuid] ?? [];

			const toggle = async (persona: WorkspacePersonaRow, value: boolean) => {
				const ctx = await ctxFor();
				if (value)
					await enableLocalPersona(
						ctx,
						persona.uuid,
						draftPermissions(persona),
					);
				else await disableLocalPersona(ctx, persona.uuid);
				await refreshPersonas();
			};

			const togglePermission = async (
				persona: WorkspacePersonaRow,
				permission: PersonaPermission,
			) => {
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
					await syncPersonasSnapshot(
						ctx,
						() => listPersonas(true, false),
						tauriPersonaCache,
					);
					await refreshPersonas();
				} catch (error) {
					syncError.value =
						error instanceof Error ? error.message : String(error);
				} finally {
					syncing.value = false;
				}
			};

			return {
				workspace,
				localOnly,
				skillLibrary,
				skillError,
				attachedLimit: ATTACHED_SKILL_LIMIT,
				attachedSkills,
				unattachedSkills,
				attachSkill,
				detachSkill,
				showAddForm,
				adding,
				addError,
				addForm,
				addLocalAgent,
				personas,
				syncing,
				syncError,
				PERSONA_PERMISSIONS,
				isEnabled,
				draftPermissions,
				toggle,
				togglePermission,
				sync,
				tokens,
				connectPersona,
				isDemo,
				localAccessAvailable,
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
