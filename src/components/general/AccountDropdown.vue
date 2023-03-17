<template>
	<div class="relative select-none mt-2 md:mt-0">
		<div
			class="cursor-pointer flex item-center gap-1 text-black dark:text-white"
			@click="isOpenProfileDropdown = !isOpenProfileDropdown"
		>
			<div class="flex items-center overflow-hidden">
				<span
					class="whitespace-nowrap text-ellipsis w-24 overflow-hidden"
					v-if="$store.getters.user?.name?.length > 15"
					>{{ $store.getters.user?.name?.slice(0, 15) }}</span
				>
				<span v-else>{{ $store.getters.user?.name }}</span>
			</div>
			<span class="material-icons text-xl">person</span>
		</div>

		<div
			v-if="isOpenProfileDropdown"
			class="absolute right-0 custom-top bg-white dark:bg-gray-900 py-2 shadow-lg z-50 border rounded dark:border-gray-700 border-gray-200"
			@mouseleave="isOpenProfileDropdown = false"
		>
			<div class="flex">
				<ul class="w-32 py-2 px-4 dark:border-gray-700 border-r-2">
					<li class="px-4 py-1 md:p-2 lg:px-4 hover:opacity-75">
						<router-link class="block" to="/profile"> Profile </router-link>
					</li>
					<li class="px-4 py-1 md:p-2 lg:px-4 hover:opacity-75">
						<router-link class="block" to="/settings"> Settings </router-link>
					</li>
					<li class="px-4 py-1 md:p-2 lg:px-4 hover:opacity-75">
						<router-link class="block" to="/archive"> Archive </router-link>
					</li>
					<li class="px-4 py-1 md:p-2 lg:px-4 hover:opacity-75">
						<a class="block" href="#" @click.prevent="logout"> Logout </a>
					</li>
				</ul>
				<div class="w-56 py-2 px-4">
					<div>
						<Select
							:options="workspaces"
							v-model="workspaceId"
							label-key="name"
							value-key="id"
							@updateSettings="updateSettings"
						/>
						<!--						<button type="button" @click="updateSettings">save</button>-->
					</div>
					<ul class="mt-4">
						<li class="py-1" v-for="{ name } in workspaceUsers">
							{{ name }}
						</li>
					</ul>
				</div>
			</div>
		</div>
	</div>
</template>

<script>
	import { logout as logoutAction } from 'src/actions/tmgr/auth';
	import {
		getWorkspaceMembers,
		getWorkspaces,
	} from 'src/actions/tmgr/workspaces';
	import {
		getUser,
		getUserSettings,
		getUserSettingsV2,
		updateUserSettings,
		updateUserSettingsV2,
	} from 'src/actions/tmgr/user';
	import Select from 'src/components/general/Select.vue';
	import Button from 'src/components/general/Button.vue';

	export default {
		name: 'AccountDropdown',
		components: {
			Button,
			Select,
		},
		emits: ['updateSettings'],
		data() {
			return {
				isOpenProfileDropdown: false,
				workspaces: [],
				user: {},
				settings: [],
				workspaceId: '',
				workspaceUsers: [],
				availableSettings: [],
			};
		},

		computed: {
			switchOn() {
				return this.$store.getters.colorScheme === 'dark';
			},
			userSettings() {
				return this.$store.getters.getUserSettings || {};
			},
		},
		async mounted() {
			this.user = await getUser();
			await this.loadSettings();
		},
		async created() {
			this.workspaces = await getWorkspaces();
		},
		methods: {
			async logout() {
				try {
					if (this.$store.getters.pusherBeamsUserId) {
						await this.$store.getters.pusherBeamsClient.stop();
						this.$store.commit('pusherBeamsUserId', null);
					}
					await logoutAction();
					await this.$store.dispatch('logout');
				} catch (e) {
					console.error(e);
				}
			},
			async loadSettings() {
				this.workspaceId = this.user.settings[0].value;
				this.workspaceUsers = await getWorkspaceMembers(this.workspaceId);
			},
			async updateSettings() {
				console.log(this.workspaceId);
				const data = await getUserSettingsV2();
				console.log('data1', data);
				this.user.settings.find(async (item) => {
					const newUserSettings = data.find(
						(element) => element.id === item.id,
					);
					newUserSettings.value = this.workspaceId;
					await updateUserSettingsV2([newUserSettings]);
					this.showAlert();
				});
			},
		},
	};
</script>

<style scoped>
	.custom-top {
		top: calc(100% + 20px);
	}
</style>
