<template>
  <div class="account relative select-none">
    <div
      class="account__user cursor-pointer flex item-center"
      :class="`${$color('navTextUser')}-500`"
      @click="isOpenProfileDropdown = !isOpenProfileDropdown"
    >
      <span>{{ $store.getters.user.name }}</span>
      <span
        class="material-icons text-lg relative"
        style="top:2px"
      >person</span>
    </div>
    <div
      v-if="isOpenProfileDropdown"
      tabindex="-1"
      class="fixed z-10 inset-0 h-full w-full bg-black opacity-50 cursor-default"
      @click="isOpenProfileDropdown = false"
    />
    <div
      v-if="isOpenProfileDropdown"
      class="absolute right-0 mt-6 py-2 rounded-lg shadow-lg z-50"
      :class="`${ $color('bg') }`"
    >
      <ul class="w-40 py-2 px-4">
        <li class="px-4 py-1 md:p-2 lg:px-4 hover:text-black">
          <router-link
            to="/settings"
            class="block"
            @click.native="isOpenProfileDropdown = false"
          >
            Settings
          </router-link>
        </li>
        <li class="px-4 py-1 md:p-2 lg:px-4 hover:text-black">
          <a
            href="#"
            class="block"
            @click.prevent="logout"
          >Logout</a>
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
				console.log('хммм')
				this.$router.push({name: 'Login'})
			}
		}
	}
</script>
