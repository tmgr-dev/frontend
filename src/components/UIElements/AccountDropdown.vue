<template>
    <div class="account relative md:ml-auto select-none">
        <div class="account__user cursor-pointer" :class="`${$color('navTextUser')}-500`" @click="isOpen = !isOpen">
            {{ $store.getters.user.name }}
            <svg xmlns="http://www.w3.org/2000/svg" class="fill-current icon" viewBox="0 0 24 24" width="18px" height="18px"><path d="M12 12c2.21 0 4-1.79 4-4s-1.79-4-4-4-4 1.79-4 4 1.79 4 4 4zm0 2c-2.67 0-8 1.34-8 4v2h16v-2c0-2.66-5.33-4-8-4z"/><path d="M0 0h24v24H0z" fill="none"/></svg>
        </div>
				<button v-if="isOpen" @click="isOpen = false" tabindex="-1"
								class="fixed z-10 inset-0 h-full w-full bg-black opacity-50 cursor-default"></button>
        <div v-if="isOpen" class="absolute right-0 mt-2 py-2 rounded-lg shadow-lg z-50" :class="`${ $color('bg') }`">
            <ul class="w-40 py-2 px-4">
							<li :class="`${$color('navTextUser')}-500`" class="flex justify-between items-center">
								<span>Mode</span>
								<DayNightSwitch :value="switchOn" @change="isOn => (isOn ? $store.commit('colorScheme', 'dark') : $store.commit('colorScheme', 'default'))"/>
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
                isOpen: false,
								switchOn: this.$store.getters.colorScheme === 'dark'
            }
        },
        methods: {
            logout () {
                this.$axios.get('auth/logout').then(() => {
                    this.removeUserData()
                }).catch(({response}) => {
                    if (response.status && response.status === 401) {
                        this.removeUserData()
                    }
                })
            },
            removeUserData () {
                this.$store.dispatch('logout')
                this.$router.push({name: 'Login'})
            }
        }
    }
</script>
