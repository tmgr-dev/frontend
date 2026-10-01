// @ts-nocheck
<template>
	<PageContainer width="narrow">
		<PageHeader
			title="Feature Settings"
			subtitle="Control which features are available in your workspace and customize your personal preferences"
		/>

		<AsyncContent
			:pending="initialPending"
			:loaded="initialLoaded"
			:error="initialError"
			:retry="loadFeatureSettings"
			label="Loading feature settings"
		>
			<div class="flex flex-col gap-6">
				<SettingsSection
					title="Personal preferences"
					description="These settings only affect your account"
				>
					<div class="divide-y divide-border">
						<SettingsRow
							v-for="feature in personalFeatures"
							:key="feature.key"
							v-slot="{ labelId, descriptionId }"
							:label="featureLabel(feature)"
							:description="featureDescription(feature.key, feature)"
						>
							<Select
								:model-value="resolveSelectValue(feature)"
								:disabled="savingToggles['user:' + feature.key]"
								@update:model-value="
									(value) => updateUserToggle(feature.key, value)
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
										v-for="option in selectOptions(feature)"
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
					v-if="currentWorkspaceId"
					:workspace-id="Number(currentWorkspaceId)"
					:is-creator="isWorkspaceOwner"
				/>

				<PagesAnalystSetting
					v-if="currentWorkspaceId && pagesEnabled"
					:workspace-id="Number(currentWorkspaceId)"
					:is-owner="isWorkspaceOwner"
				/>

				<div
					v-if="!isWorkspaceOwner && currentWorkspace"
					class="rounded-md border border-border bg-muted/50 p-4 text-sm text-ink-subtle"
				>
					You are not the owner of this workspace. Only the workspace owner can
					manage workspace features.
				</div>

				<SettingsSection
					v-if="isWorkspaceOwner"
					title="Workspace features"
					description="These settings apply to all users in your workspace"
				>
					<div class="flex flex-col gap-6">
						<div v-for="group in workspaceGroups" :key="group.name">
							<h4
								class="mb-2 text-xs font-medium uppercase tracking-wide text-ink-subtle"
							>
								{{ group.label }}
							</h4>
							<div class="divide-y divide-border">
								<SettingsRow
									v-for="feature in group.features"
									:key="feature.key"
									v-slot="{ labelId, descriptionId }"
									:label="featureLabel(feature)"
									:description="featureDescription(feature.key, feature)"
								>
									<Switch
										:checked="feature.enabled"
										:disabled="savingToggles['workspace:' + feature.key]"
										:aria-labelledby="labelId"
										:aria-describedby="descriptionId"
										@update:checked="
											(value) => updateWorkspaceToggle(feature.key, value)
										"
									/>
								</SettingsRow>
							</div>
						</div>
					</div>
				</SettingsSection>
			</div>
		</AsyncContent>
	</PageContainer>
</template>

<script>
	import {
		getUserFeatureToggles,
		getWorkspaceFeatureToggles,
	} from '@/actions/tmgr/featureToggles';
	import AsyncContent from '@/components/async/AsyncContent.vue';
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import SettingsRow from '@/components/layouts/SettingsRow.vue';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
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
	import {
		featureDescription,
		humanizeGroupName,
		humanizeKey,
	} from '@/utils/featureToggleCopy';
	import { CircleCheckBig as CircleCheckBigIcon } from 'lucide-vue-next';
	import {
		computed,
		defineComponent,
		onBeforeUnmount,
		onMounted,
		ref,
		watch,
	} from 'vue';
	import { useStore } from 'vuex';

	export default defineComponent({
		name: 'FeatureToggles',
		components: {
			AsyncContent,
			PageContainer,
			PageHeader,
			Select,
			SelectContent,
			SelectItem,
			SelectTrigger,
			SelectValue,
			SettingsRow,
			SettingsSection,
			Switch,
			PagesAnalystSetting,
			WorkspacePersonasSection,
		},
		setup() {
			const store = useStore();
			const toaster = useToast();

			const workspaceToggles = computed(
				() => store.state.featureToggles.workspaceToggles,
			);
			const userToggles = computed(
				() => store.state.featureToggles.userToggles,
			);
			const pagesEnabled = computed(
				() => !!workspaceToggles.value?.pages?.enabled,
			);
			const landingOptions = computed(() => {
				const options = ['list'];
				if (workspaceToggles.value?.board?.enabled) options.push('board');
				if (workspaceToggles.value?.dashboard?.enabled)
					options.push('dashboard');
				if (workspaceToggles.value?.daily_routines?.enabled)
					options.push('daily_routines');
				return options;
			});

			const formatOptionLabel = (option) => {
				return option
					.split('_')
					.map((word) => word.charAt(0).toUpperCase() + word.slice(1))
					.join(' ');
			};

			const featureLabel = (feature) => feature.name || humanizeKey(feature.key);

			const currentWorkspaceId = computed(() => {
				const setting = store.state.user?.settings?.find(
					(s) => s.key === 'current_workspace',
				);
				return setting?.value;
			});

			const currentWorkspace = computed(() => {
				return store.state.workspaces?.find(
					(w) => w.id == currentWorkspaceId.value,
				);
			});

			const isWorkspaceOwner = computed(() => {
				if (!currentWorkspace.value || !store.state.user) return false;
				return currentWorkspace.value.user_id === store.state.user.id;
			});

			// The backend keys each toggle by the object's own key, not by a `key`
			// field inside it — some fixtures omit that field entirely, so it must
			// never be trusted on its own (a missing key would send `{undefined: …}`).
			const entriesOf = (toggles) =>
				Object.entries(toggles || {}).map(([entryKey, feature]) => ({
					...feature,
					key: entryKey,
				}));

			const workspaceGroups = computed(() => {
				const hiddenFeatures = [
					'task.comments',
					'notifications.push',
					'notifications.email',
					'exports',
				];
				const groups = {};

				entriesOf(workspaceToggles.value).forEach((feature) => {
					if (hiddenFeatures.includes(feature.key)) return;

					if (!groups[feature.group]) {
						groups[feature.group] = {
							name: feature.group,
							label: humanizeGroupName(feature.group),
							features: [],
						};
					}
					groups[feature.group].features.push(feature);
				});

				return Object.values(groups).filter(
					(group) => group.features.length > 0,
				);
			});

			const personalFeatures = computed(() => {
				const allowedFeatures = ['default_landing_page'];
				return entriesOf(userToggles.value).filter((feature) =>
					allowedFeatures.includes(feature.key),
				);
			});

			const resolveSelectValue = (feature) => {
				if (feature.key === 'default_landing_page') {
					const allowed = landingOptions.value;
					if (allowed.includes(feature.value)) return feature.value;
					return allowed[0] || 'list';
				}
				return feature.value;
			};

			const selectOptions = (feature) => {
				if (feature.key === 'default_landing_page') {
					return landingOptions.value;
				}
				return feature.options;
			};

			const savingToggles = ref({});

			const updateWorkspaceToggle = async (key, enabled) => {
				const savingKey = 'workspace:' + key;
				if (savingToggles.value[savingKey]) return;
				savingToggles.value[savingKey] = true;
				try {
					await store.dispatch('featureToggles/updateWorkspaceToggles', {
						workspaceId: currentWorkspaceId.value,
						toggles: { [key]: enabled },
					});
					toaster.toast({
						title: 'Workspace feature updated',
						action: CircleCheckBigIcon,
						class: 'bg-green-500 border-0 text-white',
					});
				} catch (error) {
					toaster.toast({
						title: 'Error',
						description: 'Failed to update workspace feature',
						variant: 'destructive',
					});
					console.error(error);
				} finally {
					savingToggles.value[savingKey] = false;
				}
			};

			const updateUserToggle = async (key, value) => {
				const savingKey = 'user:' + key;
				if (savingToggles.value[savingKey]) return;
				savingToggles.value[savingKey] = true;
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
					toaster.toast({
						title: 'Error',
						description: 'Failed to update preference',
						variant: 'destructive',
					});
					console.error(error);
				} finally {
					savingToggles.value[savingKey] = false;
				}
			};

			const initialPending = ref(true),
				initialLoaded = ref(false),
				initialError = ref(null);
			let loadVersion = 0;
			onBeforeUnmount(() => {
				++loadVersion;
			});
			async function loadFeatureSettings() {
				const version = ++loadVersion;
				initialPending.value = true;
				initialError.value = null;
				try {
					setDocumentTitle('Feature Settings');
					if (!store.state.workspaces || store.state.workspaces.length === 0) {
						const { getWorkspaces } = await import('@/actions/tmgr/workspaces');
						const workspaces = await getWorkspaces();
						store.commit('setWorkspaces', workspaces);
					}

					const workspaceId = currentWorkspaceId.value;
					const [personal, workspace] = await Promise.all([
						getUserFeatureToggles(),
						workspaceId
							? getWorkspaceFeatureToggles(workspaceId)
							: Promise.resolve({}),
					]);
					if (
						version !== loadVersion ||
						workspaceId !== currentWorkspaceId.value
					)
						return;
					store.commit('featureToggles/setUserToggles', personal);
					store.commit('featureToggles/setWorkspaceToggles', workspace);
					// Loading is read-only; changing a preference is an explicit user action.
					if (version === loadVersion) initialLoaded.value = true;
				} catch {
					if (version === loadVersion)
						initialError.value = 'Could not load feature settings.';
				} finally {
					if (version === loadVersion) initialPending.value = false;
				}
			}
			onMounted(loadFeatureSettings);
			watch(currentWorkspaceId, () => {
				initialLoaded.value = false;
				void loadFeatureSettings();
			});

			return {
				initialPending,
				initialLoaded,
				initialError,
				loadFeatureSettings,
				currentWorkspaceId,
				currentWorkspace,
				isWorkspaceOwner,
				personalFeatures,
				workspaceGroups,
				featureLabel,
				featureDescription,
				formatOptionLabel,
				resolveSelectValue,
				selectOptions,
				savingToggles,
				updateWorkspaceToggle,
				updateUserToggle,
				pagesEnabled,
			};
		},
	});
</script>
