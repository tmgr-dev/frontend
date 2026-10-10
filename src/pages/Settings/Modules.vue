<template>
	<PageContainer width="narrow">
		<PageHeader
			title="Modules"
			:subtitle="
				currentWorkspace
					? `Workspace “${currentWorkspace.name}”: choose what is available in TMGR`
					: 'Choose what is available in TMGR'
			"
		/>

		<AsyncContent
			:pending="pending"
			:loaded="loaded"
			:error="loadError"
			:retry="load"
			label="Loading modules"
		>
			<div v-if="view" class="flex flex-col gap-6">
				<div
					v-if="!isOwner && currentWorkspace"
					class="rounded-md border border-border bg-muted/50 p-4 text-sm text-ink-subtle"
				>
					Only the workspace owner can turn modules on or off. You can hide a
					module for yourself.
				</div>

				<SettingsSection title="Core" :description="CORE_DESCRIPTION">
					<p class="text-sm text-ink" data-testid="core-line">
						{{ view.core.join(' · ') }}
					</p>
					<details
						v-if="coreRows.length"
						:open="coreRows.some((r) => r.hidden)"
						class="mt-3 text-sm"
					>
						<summary class="cursor-pointer text-ink-subtle">
							Hide core sections for me
						</summary>
						<div class="mt-2 divide-y divide-border">
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
					:title="pack.name"
					:description="`${pack.enabledCount} of ${pack.total} on`"
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
									Ask the owner to turn this on
								</span>
							</template>
							<template #default="{ labelId, descriptionId }">
								<Button
									v-if="row.canHide"
									variant="ghost"
									size="sm"
									:disabled="busy['hide:' + row.key]"
									@click="toggleHidden(row)"
								>
									{{ hideButtonLabel(row.hidden) }}
								</Button>
								<Switch
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
					title="This account"
					description="Applies to you in every workspace"
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

				<p class="text-sm text-ink-subtle" data-testid="modules-footer">
					{{ MODULES_FOOTER }}
				</p>

				<SettingsSection
					v-if="personalFeatures.length"
					title="Personal preferences"
					description="These settings only affect your account"
				>
					<div class="divide-y divide-border">
						<SettingsRow
							v-for="feature in personalFeatures"
							:key="feature.key"
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
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
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
	import PagesAnalystSetting from '@/components/workspace/PagesAnalystSetting.vue';
	import WorkspacePersonasSection from '@/components/workspace/WorkspacePersonasSection.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { featureDescription, humanizeKey } from '@/utils/featureToggleCopy';
	import { allowedLandings } from '@/utils/moduleSurfaces';
	import {
		buildModulesView,
		CORE_DESCRIPTION,
		hiddenBadge,
		hideButtonLabel,
		MODULES_FOOTER,
		payloadFromLegacy,
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
		toaster.toast({ title: 'Error', description, variant: 'destructive' });
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

	const toggleModule = (row: ModuleRow, enabled: boolean) =>
		withBusy('module:' + row.key, async () => {
			try {
				const next = await setModuleEnabled(
					currentWorkspaceId.value,
					{ key: row.key, scope: row.scope },
					enabled,
				);
				if (next) commit(next);
				else await refresh();
				toaster.toast({
					title: 'Module updated',
					action: CircleCheckBigIcon,
					class: 'bg-green-500 border-0 text-white',
				});
			} catch (error) {
				notifyError('Failed to update the module', error);
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
				notifyError('Failed to update the module', error);
			}
		});

	const updateUserPreference = (key: string, value: unknown) =>
		withBusy('user:' + key, async () => {
			try {
				await store.dispatch('featureToggles/updateUserToggles', {
					[key]: value,
				});
				toaster.toast({
					title: 'Personal preference updated',
					action: CircleCheckBigIcon,
					class: 'bg-green-500 border-0 text-white',
				});
			} catch (error) {
				notifyError('Failed to update preference', error);
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
			setDocumentTitle('Modules');
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
			if (version === loadVersion) loadError.value = 'Could not load modules.';
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
