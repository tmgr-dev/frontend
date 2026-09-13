// @ts-nocheck
<template>
	<div class="space-y-6 p-6">
		<div>
			<h2 class="text-2xl font-bold">Feature Settings</h2>
			<p class="text-gray-600 dark:text-gray-400">
				Control which features are available in your workspace and customize
				your personal preferences
			</p>
		</div>

		<AsyncContent
			:pending="initialPending"
			:loaded="initialLoaded"
			:error="initialError"
			:retry="loadFeatureSettings"
			label="Loading feature settings"
		>
			<!-- Personal Preferences -->
			<div
				class="rounded-lg border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-800"
			>
				<h3 class="mb-4 text-xl font-semibold">Personal Preferences</h3>
				<p class="mb-6 text-sm text-gray-600 dark:text-gray-400">
					These settings only affect your account
				</p>

				<div v-for="group in userGroups" :key="group.name" class="mb-6">
					<h4 class="mb-3 text-lg font-medium capitalize">{{ group.name }}</h4>
					<div class="space-y-3">
						<div
							v-for="feature in group.features"
							:key="feature.key"
							class="flex items-center gap-3 rounded-md border border-gray-200 p-3 dark:border-gray-700"
						>
							<!-- Boolean toggle -->
							<label
								v-if="feature.type === 'boolean'"
								class="relative inline-flex cursor-pointer items-center"
							>
								<input
									type="checkbox"
									:checked="feature.value"
									:disabled="savingToggles['user:' + feature.key]"
									:aria-busy="!!savingToggles['user:' + feature.key]"
									@change="updateUserToggle(feature.key, $event.target.checked)"
									class="peer sr-only"
								/>
								<div
									class="peer h-6 w-11 rounded-full bg-gray-200 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:border after:border-gray-300 after:bg-white after:transition-all after:content-[''] peer-checked:bg-blue-600 peer-checked:after:translate-x-full peer-checked:after:border-white peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-blue-300 dark:border-gray-600 dark:bg-gray-700 dark:peer-focus:ring-blue-800"
								></div>
							</label>

							<!-- Select dropdown -->
							<select
								v-else-if="feature.type === 'select'"
								:value="resolveSelectValue(feature)"
								:disabled="savingToggles['user:' + feature.key]"
								:aria-busy="!!savingToggles['user:' + feature.key]"
								@change="updateUserToggle(feature.key, $event.target.value)"
								class="rounded-md border border-gray-300 bg-white px-3 py-1 text-sm dark:border-gray-600 dark:bg-gray-700"
							>
								<option
									v-for="option in selectOptions(feature)"
									:key="option"
									:value="option"
								>
									{{ formatOptionLabel(option) }}
								</option>
							</select>

							<div>
								<p class="font-medium">{{ feature.name }}</p>
							</div>
						</div>
					</div>
				</div>
			</div>

			<!-- Workspace Features (Only for workspace owners) -->
			<div
				v-if="!isWorkspaceOwner && currentWorkspace"
				class="rounded-lg border border-yellow-200 bg-yellow-50 p-6 dark:border-yellow-700 dark:bg-yellow-900/20"
			>
				<p class="text-sm text-yellow-800 dark:text-yellow-200">
					You are not the owner of this workspace. Only the workspace owner can
					manage workspace features.
				</p>
			</div>

			<div
				v-if="isWorkspaceOwner"
				class="rounded-lg border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-800"
			>
				<h3 class="mb-4 text-xl font-semibold">Workspace Features</h3>
				<p class="mb-6 text-sm text-gray-600 dark:text-gray-400">
					These settings apply to all users in your workspace
				</p>

				<div v-for="group in workspaceGroups" :key="group.name" class="mb-6">
					<h4 class="mb-3 text-lg font-medium capitalize">{{ group.name }}</h4>
					<div class="space-y-3">
						<div
							v-for="feature in group.features"
							:key="feature.key"
							class="flex items-center gap-3 rounded-md border border-gray-200 p-3 dark:border-gray-700"
						>
							<label class="relative inline-flex cursor-pointer items-center">
								<input
									type="checkbox"
									:checked="feature.enabled"
									:disabled="savingToggles['workspace:' + feature.key]"
									:aria-busy="!!savingToggles['workspace:' + feature.key]"
									@change="
										updateWorkspaceToggle(feature.key, $event.target.checked)
									"
									class="peer sr-only"
								/>
								<div
									class="peer h-6 w-11 rounded-full bg-gray-200 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:border after:border-gray-300 after:bg-white after:transition-all after:content-[''] peer-checked:bg-blue-600 peer-checked:after:translate-x-full peer-checked:after:border-white peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-blue-300 dark:border-gray-600 dark:bg-gray-700 dark:peer-focus:ring-blue-800"
								></div>
							</label>
							<div>
								<p class="font-medium">{{ feature.name }}</p>
							</div>
						</div>
					</div>
				</div>
			</div>
		</AsyncContent>
	</div>
</template>

<script setup>
	import {
		getUserFeatureToggles,
		getWorkspaceFeatureToggles,
	} from '@/actions/tmgr/featureToggles';
	import AsyncContent from '@/components/async/AsyncContent.vue';

	import { useToast } from '@/components/ui/toast';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { CircleCheckBig as CircleCheckBigIcon } from 'lucide-vue-next';
	import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue';
	import { useStore } from 'vuex';

	const store = useStore();
	const toaster = useToast();

	const workspaceToggles = computed(
		() => store.state.featureToggles.workspaceToggles,
	);
	const userToggles = computed(() => store.state.featureToggles.userToggles);
	const landingOptions = computed(() => {
		const options = ['list'];
		if (workspaceToggles.value?.board?.enabled) options.push('board');
		if (workspaceToggles.value?.dashboard?.enabled) options.push('dashboard');
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

	const workspaceGroups = computed(() => {
		const hiddenFeatures = [
			'task.comments',
			'notifications.push',
			'notifications.email',
			'exports',
		];
		const groups = {};

		Object.values(workspaceToggles.value).forEach((feature) => {
			if (hiddenFeatures.includes(feature.key)) return;

			if (!groups[feature.group]) {
				groups[feature.group] = {
					name: feature.group,
					features: [],
				};
			}
			groups[feature.group].features.push(feature);
		});

		return Object.values(groups).filter((group) => group.features.length > 0);
	});

	const userGroups = computed(() => {
		const allowedFeatures = ['default_landing_page'];
		const groups = {};

		Object.values(userToggles.value).forEach((feature) => {
			if (!allowedFeatures.includes(feature.key)) return;

			if (!groups[feature.group]) {
				groups[feature.group] = {
					name: feature.group,
					features: [],
				};
			}
			groups[feature.group].features.push(feature);
		});

		return Object.values(groups).filter((group) => group.features.length > 0);
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
			if (version !== loadVersion || workspaceId !== currentWorkspaceId.value)
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
</script>
