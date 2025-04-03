<template>
	<div class="w-48 pr-4">
		<WorkspaceSelect />
	</div>

	<ul
		class="mt-4 flex flex-col gap-2 pt-4 md:mx-0 md:mr-4 md:mt-0 md:flex-row md:items-center md:gap-7 md:border-0 md:pt-0 lg:mr-8"
	>
		<router-link
			v-for="link in links"
			:key="link.id"
			v-slot="{ href, isActive, navigate }"
			:to="link.path"
			custom
		>
			<li :class="{ 'text-purple dark:text-green': isActive }">
				<a
					:href="href"
					:class="{ 'font-bold': isActive }"
					@click.prevent="
						() => {
							navigate();
							$emit('navigated');
						}
					"
				>
					{{ link.name }}
				</a>
			</li>
		</router-link>
	</ul>
	<button
		class="flex appearance-none items-center rounded border bg-white px-2 transition-colors duration-300 dark:bg-gray-800"
		@click="showCreatingTaskModal"
	>
		<span class="mr-2">Create</span>

		<span class="material-icons">add</span>
	</button>
</template>

<script lang="ts" setup>
	import { useStore } from 'vuex';
	import { onBeforeMount, ref, Ref, computed } from 'vue';
	import { getWorkspaces, Workspace } from '@/actions/tmgr/workspaces';
	import { getUser, updateUserSettingsV2, User } from '@/actions/tmgr/user';
	import WorkspaceSelect from '@/components/general/WorkspaceSelect.vue';
	import { generateWorkspaceUrl } from '@/utils/url';

	defineEmits(['navigated']);
	const workspaces = ref([] as Workspace[]);
	const user = ref({} as User);
	const workspaceId: Ref<number> = ref(0);

	const store = useStore();

	onBeforeMount(async () => {
		const [userData, workspaceData] = await Promise.all([
			getUser(),
			getWorkspaces(),
		]);

		user.value = userData;
		workspaces.value = workspaceData;

		const workspaceSetting = user.value.settings.find(
			(setting) => setting.key === 'current_workspace',
		);

		if (workspaceSetting) {
			workspaceId.value = +workspaceSetting.value;
		}
	});
	async function onSelectChange(workspaceId: number) {
		const settingsWithUpdatedWorkspace = user.value.settings.map((setting) => {
			if (setting.key === 'current_workspace') {
				setting.value = workspaceId;
			}

			return {
				id: setting.id,
				value: setting.value,
			};
		});

		await updateUserSettingsV2(settingsWithUpdatedWorkspace);

		store.commit('rerenderApp');
	}

	const showCreatingTaskModal = () => {
		store.commit('setShowCreatingTaskModal');
	};
	
	const currentWorkspace = computed(() => {
		return workspaces.value.find(
			(workspace) => workspace.id == workspaceId.value
		);
	});
	
	const links = computed(() => [
		{ id: 1, name: 'List', path: generateWorkspaceUrl('list', currentWorkspace.value) },
		{ id: 2, name: 'Board', path: generateWorkspaceUrl('board', currentWorkspace.value) },
		{ id: 3, name: 'Categories', path: generateWorkspaceUrl('categories', currentWorkspace.value) },
		{ id: 4, name: 'Daily Routines', path: '/routines' },
	]);
</script>
