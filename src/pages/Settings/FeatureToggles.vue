// @ts-nocheck
<template>
	<div class="space-y-6 p-6">
		<div>
			<h2 class="text-2xl font-bold">Feature Settings</h2>
			<p class="text-gray-600 dark:text-gray-400">
				Control which features are available in your workspace and customize your personal preferences
			</p>
		</div>

		<!-- Personal Preferences -->
		<div class="rounded-lg border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-800">
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
								@change="updateUserToggle(feature.key, $event.target.checked)"
								class="peer sr-only"
							/>
							<div class="peer h-6 w-11 rounded-full bg-gray-200 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:border after:border-gray-300 after:bg-white after:transition-all after:content-[''] peer-checked:bg-blue-600 peer-checked:after:translate-x-full peer-checked:after:border-white peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-blue-300 dark:border-gray-600 dark:bg-gray-700 dark:peer-focus:ring-blue-800"></div>
						</label>

						<!-- Select dropdown -->
						<select
							v-else-if="feature.type === 'select'"
							:value="resolveSelectValue(feature)"
							@change="updateUserToggle(feature.key, $event.target.value)"
							class="rounded-md border border-gray-300 bg-white px-3 py-1 text-sm dark:border-gray-600 dark:bg-gray-700"
						>
							<option v-for="option in selectOptions(feature)" :key="option" :value="option">
								{{ option.charAt(0).toUpperCase() + option.slice(1) }}
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
		<div v-if="!isWorkspaceOwner && currentWorkspace" class="rounded-lg border border-yellow-200 bg-yellow-50 p-6 dark:border-yellow-700 dark:bg-yellow-900/20">
			<p class="text-sm text-yellow-800 dark:text-yellow-200">
				You are not the owner of this workspace. Only the workspace owner can manage workspace features.
			</p>
		</div>
		
		<div v-if="isWorkspaceOwner" class="rounded-lg border border-gray-200 bg-white p-6 dark:border-gray-700 dark:bg-gray-800">
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
								@change="updateWorkspaceToggle(feature.key, $event.target.checked)"
								class="peer sr-only"
							/>
							<div class="peer h-6 w-11 rounded-full bg-gray-200 after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 after:rounded-full after:border after:border-gray-300 after:bg-white after:transition-all after:content-[''] peer-checked:bg-blue-600 peer-checked:after:translate-x-full peer-checked:after:border-white peer-focus:outline-none peer-focus:ring-4 peer-focus:ring-blue-300 dark:border-gray-600 dark:bg-gray-700 dark:peer-focus:ring-blue-800"></div>
						</label>
						<div>
							<p class="font-medium">{{ feature.name }}</p>
						</div>
					</div>
				</div>
			</div>
		</div>
	</div>
</template>

<script setup>
import { computed, onMounted } from 'vue';
import { useStore } from 'vuex';
import { useToast } from '@/components/ui/toast';
import { CircleCheckBig as CircleCheckBigIcon } from 'lucide-vue-next';
import { setDocumentTitle } from '@/composable/useDocumentTitle';

const store = useStore();
const toaster = useToast();

const workspaceToggles = computed(() => store.state.featureToggles.workspaceToggles);
const userToggles = computed(() => store.state.featureToggles.userToggles);
const landingOptions = computed(() => {
	const options = ['list'];
	if (workspaceToggles.value?.board?.enabled) options.push('board');
	if (workspaceToggles.value?.dashboard?.enabled) options.push('dashboard');
	return options;
});

const currentWorkspaceId = computed(() => {
	const setting = store.state.user?.settings?.find(s => s.key === 'current_workspace');
	return setting?.value;
});

const currentWorkspace = computed(() => {
	return store.state.workspaces?.find(w => w.id == currentWorkspaceId.value);
});

const isWorkspaceOwner = computed(() => {
	if (!currentWorkspace.value || !store.state.user) return false;
	return currentWorkspace.value.user_id === store.state.user.id;
});

const workspaceGroups = computed(() => {
	const hiddenFeatures = ['task.comments', 'task.files', 'notifications.push', 'notifications.email', 'exports'];
	const groups = {};
	
	Object.values(workspaceToggles.value).forEach((feature) => {
		if (hiddenFeatures.includes(feature.key)) return;
		
		if (!groups[feature.group]) {
			groups[feature.group] = {
				name: feature.group,
				features: []
			};
		}
		groups[feature.group].features.push(feature);
	});
	
	return Object.values(groups).filter(group => group.features.length > 0);
});

const userGroups = computed(() => {
	const allowedFeatures = ['default_landing_page'];
	const groups = {};
	
	Object.values(userToggles.value).forEach((feature) => {
		if (!allowedFeatures.includes(feature.key)) return;
		
		if (!groups[feature.group]) {
			groups[feature.group] = {
				name: feature.group,
				features: []
			};
		}
		groups[feature.group].features.push(feature);
	});
	
	return Object.values(groups).filter(group => group.features.length > 0);
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

const enforceLandingOption = async () => {
	const feature = userToggles.value?.default_landing_page;
	if (!feature) return;
	const allowed = landingOptions.value;
	if (!allowed.length) return;
	if (!allowed.includes(feature.value)) {
		// silently fix the stored value to the first allowed option
		await store.dispatch('featureToggles/updateUserToggles', {
			default_landing_page: allowed[0],
		});
	}
};

const updateWorkspaceToggle = async (key, enabled) => {
	try {
		await store.dispatch('featureToggles/updateWorkspaceToggles', {
			workspaceId: currentWorkspaceId.value,
			toggles: { [key]: enabled }
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
	}
};

const updateUserToggle = async (key, value) => {
	try {
		await store.dispatch('featureToggles/updateUserToggles', { [key]: value });
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
	}
};

onMounted(async () => {
	setDocumentTitle('Feature Settings');
	if (!store.state.workspaces || store.state.workspaces.length === 0) {
		const { getWorkspaces } = await import('@/actions/tmgr/workspaces');
		const workspaces = await getWorkspaces();
		store.commit('setWorkspaces', workspaces);
	}
	
	if (!store.getters['featureToggles/isLoaded']) {
		await store.dispatch('featureToggles/loadUserToggles');
	}
	
	if (currentWorkspaceId.value) {
		await store.dispatch('featureToggles/loadWorkspaceToggles', currentWorkspaceId.value);
	}
	
	await enforceLandingOption();
});
</script>

