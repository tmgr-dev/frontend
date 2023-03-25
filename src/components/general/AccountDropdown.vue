<template>
	<div class="relative mt-2 select-none md:mt-0">
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
			@mouseleave="isOpenProfileDropdown = false"
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
						@updateSettings="updateSettings"
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
	import {
		getUser,
		getUserSettingsV2,
		updateUserSettingsV2,
		User,
	} from 'src/actions/tmgr/user';
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
	const workspaces = ref([] as Workspace[]);
	const user = ref({} as User);
	const settings = ref([]);
	const workspaceId: Ref<number> = ref(0);
	const workspaceUsers = ref([]);

	onBeforeMount(async () => {
		workspaces.value = await getWorkspaces();
	});

	onMounted(async () => {
		await loadSettings();
	});

	async function loadSettings() {
		user.value = await getUser();
		workspaceId.value = user.value.settings[0].value;
		workspaceUsers.value = await getWorkspaceMembers(workspaceId.value);
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

	async function updateSettings() {
		const data = await getUserSettingsV2();
		user.value.settings.find(async (item) => {
			const newUserSettings = data.find((element) => element.id === item.id);
			newUserSettings.value = workspaceId.value;
			await updateUserSettingsV2([newUserSettings]);
			document.location.reload();
			isOpenProfileDropdown.value = false;
		});
	}
</script>
