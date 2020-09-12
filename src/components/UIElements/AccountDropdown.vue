<template>
	<div class="account relative md:ml-auto select-none">
		<div class="account__user cursor-pointer" :class="`${$color('navTextUser')}-500`" @click="isOpenProfileDropdown = !isOpenProfileDropdown">
			{{ $store.getters.user.name }}
			<span class="material-icons text-lg inilne-block align-text-top">person</span>
		</div>
		<button v-if="isOpenProfileDropdown" @click="isOpenProfileDropdown = false" tabindex="-1"
						class="fixed z-10 inset-0 h-full w-full bg-black opacity-50 cursor-default"></button>
		<div v-if="isOpenProfileDropdown" class="absolute right-0 mt-2 py-2 rounded-lg shadow-lg z-50" :class="`${ $color('bg') }`">
			<ul class="w-40 py-2 px-4">
				<li :class="`${$color('navTextUser')}-500`" class="flex justify-between items-center">
					<span>Mode</span>
					<DayNightSwitch :value="switchOn"
													@change="isOn => (isOn ? $store.commit('colorScheme', 'dark') : $store.commit('colorScheme', 'default'))"/>
				</li>
				<li class="px-4 py-1 md:p-2 lg:px-4">
					<a href="#" @click.prevent="logout" class="block" title="Logout">Logout</a>
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
				isOpenProfileDropdown: false
			}
		},
		computed: {
			switchOn () {
				return this.$store.getters.colorScheme === 'dark'
			}
		},
		methods: {
			async logout() {
				try {
					await this.$axios.get('auth/logout')
					this.removeUserData()
				} catch ({ response }) {
					if (response.status && response.status === 401) {
						this.removeUserData()
					}
				}
			},
			removeUserData() {
				this.$store.dispatch('logout')
				this.$router.push({name: 'Login'})
			}
		}
	}
</script>
