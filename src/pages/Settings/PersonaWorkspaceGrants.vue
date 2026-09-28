<template>
	<div class="flex flex-col gap-2">
		<div
			v-if="isDesktop"
			class="flex flex-col items-start gap-2 rounded-md border border-amber-300 bg-amber-50 p-3 text-xs text-amber-800 dark:border-amber-700 dark:bg-amber-950/40 dark:text-amber-200 sm:flex-row sm:items-center sm:justify-between"
		>
			<span>
				Local workspaces aren't listed here. Open the local workspace, then
				grant this persona in its own Personas → "On this device" panel.
			</span>
			<Button size="sm" variant="outline" class="shrink-0" @click="goToLocalPersonas">
				On this device
			</Button>
		</div>
		<p v-if="loading" class="text-xs text-muted-foreground">
			Loading workspaces…
		</p>
		<p v-else-if="workspaces.length === 0" class="text-xs text-muted-foreground">
			No shared workspaces.
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
						v-if="grantFor(ws.id)?.blocked"
						class="ml-2 text-2xs text-destructive"
					>
						Blocked by workspace creator
					</span>
				</div>
				<Switch
					:checked="!!grantFor(ws.id) && !grantFor(ws.id)?.blocked"
					:disabled="!!grantFor(ws.id)?.blocked || !!saving[ws.id]"
					@update:checked="(v) => toggle(ws.id, v)"
				/>
			</div>
			<div
				v-if="grantFor(ws.id) && !grantFor(ws.id)?.blocked"
				class="flex flex-col gap-1 border-t border-border pt-2"
			>
				<label
					v-for="perm in PERSONA_PERMISSIONS"
					:key="perm"
					class="flex items-center gap-2 text-xs"
				>
					<input
						type="checkbox"
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
				<p class="text-2xs text-muted-foreground">
					Effective here:
					{{ (grantFor(ws.id)?.effective_permissions ?? []).join(', ') || 'none' }}
				</p>
			</div>
		</div>
	</div>
</template>

<script lang="ts">
	import {
		listWorkspacePersonas,
		type Persona,
		type PersonaGrant,
		removeWorkspaceGrant,
		upsertWorkspaceGrant,
	} from '@/actions/tmgr/personas';
	import { type Workspace } from '@/actions/tmgr/workspaces';
	import { Button } from '@/components/ui/button';
	import { Switch } from '@/components/ui/switch';
	import { toast } from '@/components/ui/toast';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import { DEFAULT_GRANT_PERMISSIONS, PERSONA_PERMISSIONS } from '@/utils/personas';
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
			const loading = ref(true);

			const workspaces = computed(() =>
				((store.state.workspaces as Workspace[]) || []).filter(
					(w) => !w.is_local,
				),
			);

			const grantFor = (workspaceId: number): PersonaGrant | null =>
				grantsByWorkspace[workspaceId] ?? null;

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
									grants.find((g) => g.persona.id === props.persona.id) ??
									null;
							} catch {
								grantsByWorkspace[ws.id] = null;
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
					const status = (error as { response?: { status?: number } })
						?.response?.status;
					toast({
						title: 'Could not change this workspace',
						description:
							status === 409
								? 'Blocked by the workspace creator'
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
				const permissions = checked
					? [...current.permissions, permission]
					: current.permissions.filter((p) => p !== permission);
				saving[workspaceId] = true;
				try {
					grantsByWorkspace[workspaceId] = await upsertWorkspaceGrant(
						workspaceId,
						props.persona.id,
						permissions,
					);
				} catch {
					toast({
						title: 'Could not update permissions',
						variant: 'destructive',
					});
				} finally {
					saving[workspaceId] = false;
				}
			};

			onMounted(load);

			return {
				workspaces,
				loading,
				saving,
				grantFor,
				toggle,
				togglePermission,
				PERSONA_PERMISSIONS,
				isDesktop,
				goToLocalPersonas,
			};
		},
	});
</script>
