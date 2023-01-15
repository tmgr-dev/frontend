<template>
	<div class="relative select-none mt-2 md:mt-0">
		<div
			class="cursor-pointer flex item-center gap-1"
			:class="`${$color('inverseTextColor')}-500`"
			@click="isOpenProfileDropdown = !isOpenProfileDropdown"
		>
			<div class="flex items-center overflow-hidden">
				<span
					class="whitespace-nowrap overflow-ellipsis w-24 overflow-hidden"
					v-if="$store.getters.user?.name?.length > 15"
					>{{ $store.getters.user?.name?.slice(0, 15) }}</span
				>
				<span v-else>{{ $store.getters.user?.name }}</span>
			</div>
			<span class="material-icons text-xl">person</span>
		</div>

		<div
			v-if="isOpenProfileDropdown"
			:class="`${$color('blocks')}`"
			class="absolute right-0 custom-top py-2 shadow-lg z-50"
			@mouseleave="isOpenProfileDropdown = false"
		>
			<ul class="w-40 py-2 px-4">
				<li class="px-4 py-1 md:p-2 lg:px-4 hover:opacity-75">
					<router-link class="tc-block" to="/profile"> Profile </router-link>
				</li>
				<li class="px-4 py-1 md:p-2 lg:px-4 hover:opacity-75">
					<router-link class="tc-block" to="/settings"> Settings </router-link>
				</li>
				<li class="px-4 py-1 md:p-2 lg:px-4 hover:opacity-75">
					<router-link class="tc-block" to="/archive"> Archive </router-link>
				</li>
				<li class="px-4 py-1 md:p-2 lg:px-4 hover:opacity-75">
					<a class="tc-block" href="#" @click.prevent="logout"> Logout </a>
				</li>
			</ul>
		</div>
	</div>
</template>

<script>
	export default {
		name: 'AccountDropdown',
		data() {
			return {
				isOpenProfileDropdown: false,
			};
		},
		computed: {
			switchOn() {
				return this.$store.getters.colorScheme === 'dark';
			},
		},
		methods: {
			async logout() {
				try {
					if (this.$store.getters.pusherBeamsUserId) {
						await this.$store.getters.pusherBeamsClient.stop();
						this.$store.commit('pusherBeamsUserId', null);
					}
					await this.$axios.get('auth/logout');
					this.removeUserData();
				} catch ({ response }) {
					if (response.status && response.status === 401) {
						this.removeUserData();
					}
				}
			},
			removeUserData() {
				this.$store.dispatch('logout');
			},
		},
	};
</script>

<style scoped>
	.custom-top {
		top: calc(100% + 20px);
	}
</style>
