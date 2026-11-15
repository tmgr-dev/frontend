<template>
	<div class="flex flex-col gap-2">
		<div
			v-if="isDesktop"
			class="flex flex-col items-start gap-2 rounded-md border border-amber-300 bg-amber-50 p-3 text-xs text-amber-800 sm:flex-row sm:items-center sm:justify-between dark:border-amber-700 dark:bg-amber-950/40 dark:text-amber-200"
		>
			<span>
				{{ $t('settingsPages.personaWorkspaceGrants.localNote') }}
			</span>
			<Button
				size="sm"
				variant="outline"
				class="shrink-0"
				@click="goToLocalPersonas"
			>
				{{ $t('settingsPages.personaWorkspaceGrants.onThisDevice') }}
			</Button>
		</div>
		<p v-if="loading" class="text-xs text-muted-foreground">
			{{ $t('settingsPages.personaWorkspaceGrants.loading') }}
		</p>
		<p
			v-else-if="workspaces.length === 0"
			class="text-xs text-muted-foreground"
		>
			{{ $t('settingsPages.personaWorkspaceGrants.empty') }}
		</p>
		<div
			v-for="ws in workspaces"
			:key="ws.id"
			class="flex flex-col gap-2 rounded border border-border p-2"
		>
			<div class="flex items-center justify-between gap-2">
				<div>
					<span class="text-sm font-medium">{{ ws.name }}</span>
					<span
						v-if="persona.workspace_id === ws.id"
						class="ml-2 text-2xs text-muted-foreground"
					>
						{{ $t('settingsPages.personaWorkspaceGrants.home') }}
					</span>
					<span
						v-if="grantFor(ws.id)?.blocked"
						class="ml-2 text-2xs text-destructive"
					>
						{{ $t('settingsPages.personaWorkspaceGrants.blocked') }}
					</span>
				</div>
				<Switch
					:checked="!!grantFor(ws.id) && !grantFor(ws.id)?.blocked"
					:disabled="
						!!grantFor(ws.id)?.blocked ||
						!!saving[ws.id] ||
						persona.workspace_id === ws.id
					"
					@update:checked="(v) => toggle(ws.id, v)"
				/>
			</div>
			<div
				v-if="grantFor(ws.id) && !grantFor(ws.id)?.blocked"
				class="flex flex-col gap-1 border-t border-border pt-2"
			>
				<label
					v-for="perm in LISTED_PERMISSIONS"
					:key="perm"
					class="flex items-center gap-2 text-xs"
				>
					<input
						type="checkbox"
						class="accent-primary"
						:checked="grantFor(ws.id)?.permissions.includes(perm)"
						:disabled="!!saving[ws.id]"
						@change="
							(e) =>
								togglePermission(
									ws.id,
									perm,
									(e.target as HTMLInputElement).checked,
								)
						"
					/>
					{{ perm }}
				</label>
				<template v-if="chatsOn[ws.id]">
					<label class="flex items-center gap-2 text-xs">
						<Switch
							:checked="!!grantFor(ws.id)?.permissions.includes(CHATS_REPLY)"
							:disabled="!!saving[ws.id]"
							@update:checked="(v) => togglePermission(ws.id, CHATS_REPLY, v)"
						/>
						{{ $t('settingsPages.personaWorkspaceGrants.allowInChats') }}
					</label>
					<p
						v-if="grantFor(ws.id)?.permissions.includes(CHATS_REPLY)"
						class="text-2xs text-amber-700 dark:text-amber-300"
					>
						{{ $t('settingsPages.personaWorkspaceGrants.chatsWarning') }}
					</p>
					<label class="flex items-center gap-2 text-xs">
						<Switch
							:checked="!!grantFor(ws.id)?.permissions.includes(CHATS_READ)"
							:disabled="!!saving[ws.id]"
							@update:checked="(v) => togglePermission(ws.id, CHATS_READ, v)"
						/>
						{{ $t('settingsPages.personaWorkspaceGrants.readChats') }}
					</label>
					<label class="flex items-center gap-2 text-xs">
						<Switch
							:checked="!!grantFor(ws.id)?.permissions.includes(CHATS_WRITE)"
							:disabled="!!saving[ws.id]"
							@update:checked="(v) => togglePermission(ws.id, CHATS_WRITE, v)"
						/>
						{{ $t('settingsPages.personaWorkspaceGrants.writeChats') }}
					</label>
					<label class="flex flex-col gap-1 text-xs">
						{{ $t('settingsPages.personaWorkspaceGrants.chatReplies') }}
						<select
							class="h-8 w-fit rounded-md border border-border bg-background px-2 text-xs"
							data-testid="persona-chat-mode"
							:value="grantFor(ws.id)?.chat_mode ?? 'server'"
							:disabled="!!saving[ws.id]"
							@change="
								(e) =>
									changeChatMode(
										ws.id,
										(e.target as HTMLSelectElement).value as PersonaChatMode,
									)
							"
						>
							<option value="server">
								{{ $t('settingsPages.personaWorkspaceGrants.chatModeServer') }}
							</option>
							<option value="agent">
								{{ $t('settingsPages.personaWorkspaceGrants.chatModeAgent') }}
							</option>
						</select>
					</label>
					<p class="text-2xs text-muted-foreground">
						{{ $t('settingsPages.personaWorkspaceGrants.chatModeHelp') }}
					</p>
				</template>
				<p class="text-2xs text-muted-foreground">
					{{
						$t('settingsPages.personaWorkspaceGrants.effective', {
							permissions:
								(grantFor(ws.id)?.effective_permissions ?? []).join(', ') ||
								$t('common.states.none').toLowerCase(),
						})
					}}
				</p>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import {
		listWorkspacePersonas,
		PersonaChatModeError,
		removeWorkspaceGrant,
		setPersonaChatMode,
		upsertWorkspaceGrant,
		type Persona,
		type PersonaChatMode,
		type PersonaGrant,
	} from '@/actions/tmgr/personas';
	import { getWorkspaceModules } from '@/actions/tmgr/modules';
	import { type Workspace } from '@/actions/tmgr/workspaces';
	import { Button } from '@/components/ui/button';
	import { Switch } from '@/components/ui/switch';
	import { toast } from '@/components/ui/toast';
	import { t } from '@/i18n';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import {
		chatsModuleOn,
		DEFAULT_GRANT_PERMISSIONS,
		PERSONA_PERMISSIONS,
		togglePermissionList,
	} from '@/utils/personas';
	import {
		computed,
		defineComponent,
		onMounted,
		reactive,
		ref,
		type PropType,
	} from 'vue';

	export default defineComponent({
		name: 'PersonaWorkspaceGrants',
		components: { Button, Switch },
		props: {
			persona: { type: Object as PropType<Persona>, required: true },
		},
		setup(props) {
			const grantsByWorkspace = reactive<Record<number, PersonaGrant | null>>(
				{},
			);
			const saving = reactive<Record<number, boolean>>({});
			const chatsOn = reactive<Record<number, boolean>>({});
			const loading = ref(true);

			const workspaces = computed(() =>
				((store.state.workspaces as Workspace[]) || []).filter(
					(w) =>
						!w.is_local &&
						(!props.persona.workspace_id ||
							w.id === props.persona.workspace_id),
				),
			);

			const grantFor = (workspaceId: number): PersonaGrant | null =>
				grantsByWorkspace[workspaceId] ?? null;

			const CHATS_REPLY = 'chats:reply';
			const CHATS_READ = 'chats:read';
			const CHATS_WRITE = 'chats:write';
			const LISTED_PERMISSIONS = PERSONA_PERMISSIONS.filter(
				(p) => p !== CHATS_REPLY && p !== CHATS_READ && p !== CHATS_WRITE,
			);

			const isDesktop = isDesktopApp();

			const goToLocalPersonas = () => {
				document
					.getElementById('local-personas-panel')
					?.scrollIntoView({ behavior: 'smooth', block: 'start' });
			};

			const load = async () => {
				loading.value = true;
				try {
					await Promise.all(
						workspaces.value.map(async (ws) => {
							try {
								const grants = await listWorkspacePersonas(ws.id);
								grantsByWorkspace[ws.id] =
									grants.find((g) => g.persona.id === props.persona.id) ?? null;
							} catch {
								grantsByWorkspace[ws.id] = null;
							}
							try {
								chatsOn[ws.id] = chatsModuleOn(
									await getWorkspaceModules(ws.id),
								);
							} catch {
								chatsOn[ws.id] = false;
							}
						}),
					);
				} finally {
					loading.value = false;
				}
			};

			const toggle = async (workspaceId: number, enabled: boolean) => {
				saving[workspaceId] = true;
				try {
					if (enabled) {
						grantsByWorkspace[workspaceId] = await upsertWorkspaceGrant(
							workspaceId,
							props.persona.id,
							DEFAULT_GRANT_PERMISSIONS,
						);
					} else {
						await removeWorkspaceGrant(workspaceId, props.persona.id);
						grantsByWorkspace[workspaceId] = null;
					}
				} catch (error) {
					const status = (error as { response?: { status?: number } })?.response
						?.status;
					toast({
						title: t('settingsPages.personaWorkspaceGrants.changeFailed'),
						description:
							status === 409
								? t('settingsPages.personaWorkspaceGrants.blockedByCreator')
								: undefined,
						variant: 'destructive',
					});
				} finally {
					saving[workspaceId] = false;
				}
			};

			const togglePermission = async (
				workspaceId: number,
				permission: string,
				checked: boolean,
			) => {
				const current = grantFor(workspaceId);
				if (!current) return;
				const permissions = togglePermissionList(
					current.permissions,
					permission,
					checked,
				);
				saving[workspaceId] = true;
				try {
					grantsByWorkspace[workspaceId] = await upsertWorkspaceGrant(
						workspaceId,
						props.persona.id,
						permissions,
					);
				} catch {
					toast({
						title: t('settingsPages.personaWorkspaceGrants.permissionsFailed'),
						variant: 'destructive',
					});
				} finally {
					saving[workspaceId] = false;
				}
			};

			const changeChatMode = async (
				workspaceId: number,
				mode: PersonaChatMode,
			) => {
				const current = grantFor(workspaceId);
				if (!current) return;
				saving[workspaceId] = true;
				try {
					await setPersonaChatMode(props.persona.id, workspaceId, mode);
					grantsByWorkspace[workspaceId] = { ...current, chat_mode: mode };
				} catch (error) {
					grantsByWorkspace[workspaceId] = { ...current };
					toast({
						title: t('settingsPages.personaWorkspaceGrants.chatModeFailed'),
						description:
							error instanceof PersonaChatModeError &&
							error.code === 'persona_agent_needs_grants'
								? t('settingsPages.personaWorkspaceGrants.chatModeNeedsGrants')
								: undefined,
						variant: 'destructive',
					});
				} finally {
					saving[workspaceId] = false;
				}
			};

			onMounted(load);

			return {
				workspaces,
				chatsOn,
				loading,
				saving,
				grantFor,
				toggle,
				togglePermission,
				changeChatMode,
				LISTED_PERMISSIONS,
				CHATS_REPLY,
				CHATS_READ,
				CHATS_WRITE,
				isDesktop,
				goToLocalPersonas,
			};
		},
	});
</script>
