<template>
	<div class="relative mt-2 select-none md:mt-0" ref="$wrapper">
		<div
			class="item-center flex cursor-pointer gap-1 text-black dark:text-white"
			@click="isOpenProfileDropdown = !isOpenProfileDropdown"
		>
			<div class="flex items-center overflow-hidden">
				<span
					class="w-24 overflow-hidden text-ellipsis whitespace-nowrap"
					v-if="store.getters.user?.name?.length > 15"
				>
					{{ store.getters.user?.name?.slice(0, 15) }}
				</span>

				<span v-else>{{ store.getters.user?.name }}</span>
			</div>

			<span class="material-icons text-xl">person</span>
		</div>

		<!--	dropdown block	-->
		<div
			v-if="isOpenProfileDropdown"
			class="absolute top-[calc(100%+20px)] right-0 z-50 rounded border border-gray-200 bg-white py-2 shadow-lg dark:border-gray-700 dark:bg-gray-900"
		>
			<div class="sm:flex">
				<ul
					class="border-b-2 py-2 px-4 dark:border-gray-700 sm:w-32 sm:border-r-2 sm:border-b-0"
				>
					<li
						v-for="link in links"
						:key="link.id"
						class="py-1 hover:opacity-75 md:p-2 lg:px-4"
					>
						<router-link class="block" :to="link.to">
							{{ link.name }}
						</router-link>
					</li>
					<li class="py-1 hover:opacity-75 md:p-2 lg:px-4">
						<a class="block" href="#" @click.prevent="logout">Logout</a>
					</li>
				</ul>

				<div class="w-56 py-2 px-4">
					<Select
						:options="workspaces"
						v-model="workspaceId"
						label-key="name"
						value-key="id"
						@change="onSelectChange"
					/>

					<div class="mt-2 text-neutral-400">workspace users:</div>

					<ul class="h-24 overflow-y-auto">
						<li class="py-1 sm:px-0" v-for="{ name } in workspaceUsers">
							{{ name }}
						</li>
					</ul>
				</div>
			</div>
		</div>
	</div>
</template>

<script setup lang="ts">
	import { logout as logoutAction } from 'src/actions/tmgr/auth';
	import {
		getWorkspaceMembers,
		getWorkspaces,
		Workspace,
	} from 'src/actions/tmgr/workspaces';
	import { getUser, updateUserSettingsV2, User } from 'src/actions/tmgr/user';
	import Select from 'src/components/general/Select.vue';
	import { onBeforeMount, onMounted, Ref, ref } from 'vue';
	import store from 'src/store';

	const emit = defineEmits(['updateSettings']);
	const isOpenProfileDropdown = ref(false);
	const links = [
		{
			id: 1,
			name: 'Profile',
			to: '/profile',
		},
		{
			id: 2,
			name: 'Settings',
			to: '/settings',
		},
		{
			id: 3,
			name: 'Archive',
			to: '/archive',
		},
	];
	const $wrapper: Ref<HTMLDivElement | null> = ref(null);
	const workspaces = ref([] as Workspace[]);
	const user = ref({} as User);
	const workspaceId: Ref<number> = ref(0);
	const workspaceUsers = ref([]);

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
			workspaceUsers.value = await getWorkspaceMembers(workspaceId.value);
		}
	});

	onMounted(async () => {
		document.addEventListener('click', (e: MouseEvent) => {
			if (!$wrapper.value?.contains(e.target as Node)) {
				isOpenProfileDropdown.value = false;
			}
		});
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

		store.commit('appRerender')
	}

	async function logout() {
		try {
			if (store.getters.pusherBeamsUserId) {
				await store.getters.pusherBeamsClient.stop();
				store.commit('pusherBeamsUserId', null);
			}
			await logoutAction();
			await store.dispatch('logout');
		} catch (e) {
			console.error(e);
		}
	}
</script>
