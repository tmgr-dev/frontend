<template>
	<PageContainer width="narrow">
		<PageHeader
			:title="$t('settingsPages.modules.title')"
			:subtitle="
				currentWorkspace
					? $t('settingsPages.modules.subtitleWorkspace', {
							name: currentWorkspace.name,
						})
					: $t('settingsPages.modules.subtitle')
			"
		/>

		<AsyncContent
			:pending="pending"
			:loaded="loaded"
			:error="loadError"
			:retry="load"
			:label="$t('settingsPages.modules.loading')"
		>
			<div v-if="view" class="flex flex-col gap-6">
				<div
					v-if="!isOwner && currentWorkspace"
					class="rounded-md border border-border bg-muted/50 p-4 text-sm text-ink-subtle"
				>
					{{ $t('settingsPages.modules.ownerOnlyNotice') }}
				</div>

				<div
					v-if="presets.length"
					data-tour="modules.presets"
					class="flex items-center gap-3"
				>
					<Button
						variant="outline"
						size="sm"
						:disabled="!canApplyPreset(isOwner)"
						data-testid="apply-preset"
						@click="presetOpen = true"
					>
						{{ $t('settingsPages.modules.applyPreset') }}
					</Button>
					<span
						v-if="!canApplyPreset(isOwner)"
						class="text-sm text-ink-subtle"
						data-testid="apply-preset-hint"
						>{{ $t('settingsPages.modules.ownerOnlyHint') }}</span
					>
				</div>
				<ModulesPresetDialog
					v-if="payload && currentWorkspaceId"
					v-model:open="presetOpen"
					:workspace-id="currentWorkspaceId"
					:presets="presets"
					:catalog="payload.modules"
					@applied="commit"
				/>

				<SettingsSection
					:title="$t('settingsPages.modules.core')"
					:description="$t('settingsPages.helpers.modules.coreDescription')"
				>
					<p class="text-sm text-ink" data-testid="core-line">
						<template v-for="(item, index) in view.coreItems" :key="item.name">
							<span v-if="index" class="text-ink-subtle"> · </span>
							<span
								:class="item.off ? 'text-ink-subtle opacity-60' : ''"
								:data-testid="`core-item-${item.name}`"
								:data-off="item.off || undefined"
								>{{ item.name
								}}<span v-if="item.needs" class="ml-1 text-xs"
									>({{ item.needs }})</span
								></span
							>
						</template>
					</p>
					<details
						v-if="coreRows.length"
						:open="coreRows.some((r) => r.hidden)"
						class="mt-3 text-sm"
					>
						<summary class="cursor-pointer text-ink-subtle">
							{{ $t('settingsPages.modules.hideCore') }}
						</summary>
						<div class="divide-y divide-border mt-2">
							<SettingsRow
								v-for="row in coreRows"
								:key="row.key"
								:label="row.name"
							>
								<template #description>
									{{ row.description }}
									<span
										v-if="hiddenBadge(row.hidden)"
										class="ml-1 rounded bg-muted px-1.5 py-0.5 text-xs text-ink-subtle"
										>{{ hiddenBadge(row.hidden) }}</span
									>
								</template>
								<Button
									variant="ghost"
									size="sm"
									:disabled="busy['hide:' + row.key]"
									@click="toggleHidden(row)"
								>
									{{ hideButtonLabel(row.hidden) }}
								</Button>
							</SettingsRow>
						</div>
					</details>
				</SettingsSection>

				<SettingsSection
					v-for="pack in view.packs"
					:key="pack.key"
					data-tour="modules.list"
					:title="pack.name"
					:description="
						$t('settingsPages.modules.packCount', {
							enabled: pack.enabledCount,
							total: pack.total,
						})
					"
				>
					<div class="divide-y divide-border">
						<SettingsRow
							v-for="row in pack.rows"
							:key="row.key"
							:label="row.name"
						>
							<template #description>
								{{ row.description }}
								<span
									v-if="hiddenBadge(row.hidden)"
									class="ml-1 rounded bg-muted px-1.5 py-0.5 text-xs text-ink-subtle"
									>{{ hiddenBadge(row.hidden) }}</span
								>
								<span
									v-if="row.clients || row.needs"
									class="mt-0.5 block text-xs"
								>
									{{ [row.clients, row.needs].filter(Boolean).join(' · ') }}
								</span>
								<span v-if="row.askOwner" class="mt-0.5 block text-xs">
									{{ $t('settingsPages.modules.askOwner') }}
								</span>
							</template>
							<template v-if="localOnly && row.needsAccount" #default>
								<RequiresAccount compact />
							</template>
							<template v-else #default="{ labelId, descriptionId }">
								<Button
									v-if="row.canHide"
									data-tour="modules.hide"
									variant="ghost"
									size="sm"
									:disabled="busy['hide:' + row.key]"
									@click="toggleHidden(row)"
								>
									{{ hideButtonLabel(row.hidden) }}
								</Button>
								<Switch
									:data-tour="row.key === 'graph' ? 'modules.graph' : undefined"
									:checked="row.enabled"
									:disabled="!row.canToggle || busy['module:' + row.key]"
									:aria-labelledby="labelId"
									:aria-describedby="descriptionId"
									@update:checked="(value) => toggleModule(row, value)"
								/>
							</template>
						</SettingsRow>
					</div>
				</SettingsSection>

				<SettingsSection
					v-if="view.account.length"
					:title="$t('settingsPages.modules.account')"
					:description="$t('settingsPages.modules.accountDescription')"
				>
					<div class="divide-y divide-border">
						<SettingsRow
							v-for="row in view.account"
							:key="row.key"
							v-slot="{ labelId, descriptionId }"
							:label="row.name"
							:description="row.description"
						>
							<Switch
								:checked="row.enabled"
								:disabled="busy['module:' + row.key]"
								:aria-labelledby="labelId"
								:aria-describedby="descriptionId"
								@update:checked="(value) => toggleModule(row, value)"
							/>
						</SettingsRow>
					</div>
				</SettingsSection>

				<AlertDialog v-model:open="confirmOpen">
					<AlertDialogContent>
						<AlertDialogHeader>
							<AlertDialogTitle>{{
								$t('settingsPages.modules.turnOffTitle')
							}}</AlertDialogTitle>
							<AlertDialogDescription data-testid="workflow-off-confirm">
								{{ confirmMessage }}
							</AlertDialogDescription>
						</AlertDialogHeader>
						<AlertDialogFooter>
							<AlertDialogCancel @click="pendingRow = null">{{
								$t('common.actions.cancel')
							}}</AlertDialogCancel>
							<AlertDialogAction @click="confirmToggle">
								{{ $t('settingsPages.modules.turnOff') }}
							</AlertDialogAction>
						</AlertDialogFooter>
					</AlertDialogContent>
				</AlertDialog>

				<p class="text-sm text-ink-subtle" data-testid="modules-footer">
					{{ $t('settingsPages.helpers.modules.footer') }}
				</p>

				<SettingsSection
					v-if="personalFeatures.length"
					:title="$t('settingsPages.modules.personal')"
					:description="$t('settingsPages.modules.personalDescription')"
				>
					<div class="divide-y divide-border">
						<SettingsRow
							v-for="feature in personalFeatures"
							:key="feature.key"
							:data-tour="
								feature.key === 'default_landing_page'
									? 'settings.default-landing'
									: undefined
							"
							v-slot="{ labelId, descriptionId }"
							:label="feature.name || humanizeKey(feature.key)"
							:description="featureDescription(feature.key, feature)"
						>
							<Select
								:model-value="resolveSelectValue(feature)"
								:disabled="busy['user:' + feature.key]"
								@update:model-value="
									(value) => updateUserPreference(feature.key, value)
								"
							>
								<SelectTrigger
									class="w-full sm:w-48"
									:aria-labelledby="labelId"
									:aria-describedby="descriptionId"
								>
									<SelectValue />
								</SelectTrigger>
								<SelectContent>
									<SelectItem
										v-for="option in landingOptions"
										:key="option"
										:value="option"
									>
										{{ formatOptionLabel(option) }}
									</SelectItem>
								</SelectContent>
							</Select>
						</SettingsRow>
					</div>
				</SettingsSection>

				<WorkspacePersonasSection
					v-if="currentWorkspaceId && isFeatureEnabled('personas')"
					:workspace-id="Number(currentWorkspaceId)"
					:is-creator="isOwner"
				/>

				<AgentRunsSetting
					v-if="currentWorkspaceId && Number(currentWorkspaceId) > 0"
					:workspace-id="Number(currentWorkspaceId)"
				/>

				<AiWriteModeSetting
					v-if="currentWorkspaceId && Number(currentWorkspaceId) > 0"
					:workspace-id="Number(currentWorkspaceId)"
					:is-owner="isOwner"
				/>

				<PagesAnalystSetting
					v-if="
						currentWorkspaceId &&
						isFeatureEnabled('pages') &&
						isFeatureEnabled('personas')
					"
					:workspace-id="Number(currentWorkspaceId)"
					:is-owner="isOwner"
				/>
			</div>
		</AsyncContent>
	</PageContainer>
</template>

<script setup lang="ts">
	// @ts-nocheck
	import { getUserFeatureToggles } from '@/actions/tmgr/featureToggles';
	import {
		getLegacyWorkspaceFeatureToggles,
		getWorkspaceModules,
		hideModuleForMe,
		setModuleEnabled,
	} from '@/actions/tmgr/modules';
	import AsyncContent from '@/components/async/AsyncContent.vue';
	import ModulesPresetDialog from '@/components/general/ModulesPresetDialog.vue';
	import RequiresAccount from '@/components/general/RequiresAccount.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import {
		AlertDialog,
		AlertDialogAction,
		AlertDialogCancel,
		AlertDialogContent,
		AlertDialogDescription,
		AlertDialogFooter,
		AlertDialogHeader,
		AlertDialogTitle,
	} from '@/components/ui/alert-dialog';
	import { Button } from '@/components/ui/button';
	import {
		Select,
		SelectContent,
		SelectItem,
		SelectTrigger,
		SelectValue,
	} from '@/components/ui/select';
	import { Switch } from '@/components/ui/switch';
	import { useToast } from '@/components/ui/toast';
	import AgentRunsSetting from '@/components/workspace/AgentRunsSetting.vue';
	import AiWriteModeSetting from '@/components/workspace/AiWriteModeSetting.vue';
	import PagesAnalystSetting from '@/components/workspace/PagesAnalystSetting.vue';
	import WorkspacePersonasSection from '@/components/workspace/WorkspacePersonasSection.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { t } from '@/i18n';
	import { featureDescription, humanizeKey } from '@/utils/featureToggleCopy';
	import { visiblePresets } from '@/utils/localOnly';
	import { allowedLandings } from '@/utils/moduleSurfaces';
	import {
		buildModulesView,
		canApplyPreset,
		hiddenBadge,
		hideButtonLabel,
		payloadFromLegacy,
		payloadNotices,
		workflowOffConfirm,
		type ModuleRow,
		type ModulesPayload,
	} from '@/utils/modules';
	import { CircleCheckBig as CircleCheckBigIcon } from 'lucide-vue-next';
	import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
	import { useStore } from 'vuex';

	const store = useStore();
	const toaster = useToast();
	const { isFeatureEnabled } = useFeatureToggles();

	const LEGACY_HIDDEN_KEYS = [
		'task.comments',
		'notifications.push',
		'notifications.email',
		'exports',
	];

	const payload = ref<ModulesPayload | null>(null);
	const pending = ref(true);
	const loaded = ref(false);
	const loadError = ref<string | null>(null);
	const busy = ref<Record<string, boolean>>({});

	const currentWorkspaceId = computed(
		() =>
			store.state.user?.settings?.find(
				(s: any) => s.key === 'current_workspace',
			)?.value as string | number,
	);
	const currentWorkspace = computed(() =>
		store.state.workspaces?.find((w: any) => w.id == currentWorkspaceId.value),
	);
	const ownsWorkspace = computed(
		() =>
			!!currentWorkspace.value?.is_local ||
			(!!currentWorkspace.value &&
				currentWorkspace.value.user_id === store.state.user?.id),
	);
	const isOwner = computed(() => !!payload.value?.canManage);
	const localOnly = computed(() => store.getters.isLocalOnly === true);
	const presets = computed(() =>
		visiblePresets(payload.value?.presets ?? [], localOnly.value),
	);

	const view = computed(() =>
		payload.value ? buildModulesView(payload.value, isOwner.value) : null,
	);
	const coreRows = computed(() =>
		(payload.value?.canHide ? payload.value.modules : [])
			.filter((m) => m.core && m.scope !== 'user')
			.map((m) => ({
				key: m.key,
				name: m.name || humanizeKey(m.key),
				description: m.description || '',
				hidden: m.hidden === true,
			})),
	);

	const userToggles = computed(
		() => (store.state as any).featureToggles.userToggles,
	);
	const landingOptions = computed(() => allowedLandings(isFeatureEnabled));
	const personalFeatures = computed(() =>
		Object.entries(userToggles.value || {})
			.filter(([key]) => key === 'default_landing_page')
			.map(([key, feature]: [string, any]) => ({ ...feature, key })),
	);
	const resolveSelectValue = (feature: any) =>
		landingOptions.value.includes(feature.value)
			? feature.value
			: landingOptions.value[0];
	const formatOptionLabel = (option: string) =>
		option
			.split('_')
			.map((word) => word.charAt(0).toUpperCase() + word.slice(1))
			.join(' ');

	const presetOpen = ref(false);

	const commit = (next: ModulesPayload) => {
		payload.value = next;
		store.commit('featureToggles/setWorkspaceModules', {
			workspaceId: currentWorkspaceId.value,
			payload: next,
		});
	};

	const refresh = async () => {
		const workspaceId = currentWorkspaceId.value;
		const served = await getWorkspaceModules(workspaceId);
		if (served) return commit(served);
		const map = await getLegacyWorkspaceFeatureToggles(workspaceId);
		store.commit('featureToggles/setWorkspaceToggles', map);
		payload.value = payloadFromLegacy(
			map,
			ownsWorkspace.value,
			LEGACY_HIDDEN_KEYS,
		);
	};

	const notifyError = (description: string, error: unknown) => {
		toaster.toast({
			title: t('settingsPages.modules.errorTitle'),
			description,
			variant: 'destructive',
		});
		console.error(error);
	};

	const withBusy = async (key: string, run: () => Promise<void>) => {
		if (busy.value[key]) return;
		busy.value[key] = true;
		try {
			await run();
		} finally {
			busy.value[key] = false;
		}
	};

	const confirmOpen = ref(false);
	const pendingRow = ref<ModuleRow | null>(null);
	const confirmMessage = ref('');

	const toggleModule = (row: ModuleRow, enabled: boolean) => {
		const message = workflowOffConfirm(row.key, enabled);
		if (!message) return applyToggle(row, enabled);
		pendingRow.value = row;
		confirmMessage.value = message;
		confirmOpen.value = true;
	};

	const confirmToggle = () => {
		const row = pendingRow.value;
		pendingRow.value = null;
		if (row) void applyToggle(row, false);
	};

	const applyToggle = (row: ModuleRow, enabled: boolean) =>
		withBusy('module:' + row.key, async () => {
			try {
				const next = await setModuleEnabled(
					currentWorkspaceId.value,
					{ key: row.key, scope: row.scope },
					enabled,
				);
				if (next) {
					commit(next);
					payloadNotices(next).forEach((title) => toaster.toast({ title }));
				} else await refresh();
				toaster.toast({
					title: t('settingsPages.modules.updated'),
					action: CircleCheckBigIcon,
					class: 'bg-green-500 border-0 text-white',
				});
			} catch (error) {
				notifyError(t('settingsPages.modules.updateFailed'), error);
				await refresh().catch(() => undefined);
			}
		});

	const toggleHidden = (row: { key: string; hidden: boolean }) =>
		withBusy('hide:' + row.key, async () => {
			try {
				commit(
					await hideModuleForMe(currentWorkspaceId.value, row.key, !row.hidden),
				);
			} catch (error) {
				notifyError(t('settingsPages.modules.updateFailed'), error);
			}
		});

	const updateUserPreference = (key: string, value: unknown) =>
		withBusy('user:' + key, async () => {
			try {
				await store.dispatch('featureToggles/updateUserToggles', {
					[key]: value,
				});
				toaster.toast({
					title: t('settingsPages.modules.personalUpdated'),
					action: CircleCheckBigIcon,
					class: 'bg-green-500 border-0 text-white',
				});
			} catch (error) {
				notifyError(t('settingsPages.modules.preferenceFailed'), error);
			}
		});

	let loadVersion = 0;
	onBeforeUnmount(() => {
		++loadVersion;
	});

	async function load() {
		const version = ++loadVersion;
		pending.value = true;
		loadError.value = null;
		try {
			setDocumentTitle(t('settingsPages.modules.title'));
			if (!store.state.workspaces || store.state.workspaces.length === 0) {
				const { getWorkspaces } = await import('@/actions/tmgr/workspaces');
				store.commit('setWorkspaces', await getWorkspaces());
			}
			const workspaceId = currentWorkspaceId.value;
			const [, personal] = await Promise.all([
				workspaceId ? refresh() : Promise.resolve(),
				getUserFeatureToggles(),
			]);
			if (version !== loadVersion || workspaceId !== currentWorkspaceId.value)
				return;
			store.commit('featureToggles/setUserToggles', personal);
			loaded.value = true;
		} catch {
			if (version === loadVersion)
				loadError.value = t('settingsPages.modules.loadFailed');
		} finally {
			if (version === loadVersion) pending.value = false;
		}
	}

	onMounted(load);
	watch(currentWorkspaceId, () => {
		loaded.value = false;
		void load();
	});
</script>
