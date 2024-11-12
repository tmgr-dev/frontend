<template>
	<div class="w-48 pr-4">
		<Select
			:options="workspaces"
			v-model="selectedWorkspaceId"
			label-key="name"
			value-key="id"
			@change="handleWorkspaceChange"
			:loading="isLoading"
		/>
	</div>
</template>

<script lang="ts" setup>
	import { ref, onBeforeMount } from 'vue';
	import { useStore } from 'vuex';
	import Select from '@/components/general/Select.vue';
	import { getWorkspaces, Workspace } from '@/actions/tmgr/workspaces';
	import { getUser, updateUserSettingsV2, User } from '@/actions/tmgr/user';

	// State
	const store = useStore();
	const workspaces = ref<Workspace[]>([]);
	const selectedWorkspaceId = ref<number>(0);
	const isLoading = ref(true);
	const user = ref<User>({} as User);

	// Methods
	const handleWorkspaceChange = async (workspaceId: number) => {
		try {
			const settingsWithUpdatedWorkspace = user.value.settings.map(
				(setting) => {
					if (setting.key === 'current_workspace') {
						setting.value = workspaceId;
					}
					return {
						id: setting.id,
						value: setting.value,
					};
				},
			);

			await updateUserSettingsV2(settingsWithUpdatedWorkspace);
			store.commit('rerenderApp');
		} catch (error) {
			console.error('Failed to update workspace:', error);
			// You might want to add error handling here
		}
	};

	// Lifecycle
	onBeforeMount(async () => {
		try {
			isLoading.value = true;
			const [userData, workspaceData] = await Promise.all([
				getUser(),
				getWorkspaces(),
			]);

			user.value = userData;
			workspaces.value = workspaceData;

			// Set initial workspace from user settings
			const workspaceSetting = userData.settings.find(
				(setting: any) => setting.key === 'current_workspace',
			);

			if (workspaceSetting) {
				selectedWorkspaceId.value = +workspaceSetting.value;
			}
		} catch (error) {
			console.error('Failed to initialize workspace select:', error);
			// You might want to add error handling here
		} finally {
			isLoading.value = false;
		}
	});
</script>
