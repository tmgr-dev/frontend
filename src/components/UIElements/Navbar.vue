<template>
	<nav :class="`${$color('blocks')} shadow`" role="navigation">
		<div class="container mx-auto p-4 flex flex-wrap items-center md:flex-no-wrap">
			<div class="ml-auto md:hidden">
				<button class="flex items-center px-3 py-2 border rounded" type="button" @click="isHidden = !isHidden">
					Menu
				</button>
			</div>
			<div class="w-full md:w-auto md:flex-grow md:flex md:items-center md:block" :class="{ hidden: isHidden }">
				<ul
					class="flex flex-col mt-4 -mx-4 pt-4 border-t md:flex-row md:items-center md:mx-0 md:mt-0 md:pt-0 md:mr-4 lg:mr-8 md:border-0">
					<li>
						<router-link to="/"
							:class="`block px-4 py-1 md:p-2 lg:px-4 ${$route.path === '/' ? $color('navLinkFocused') + '-600' : ''}`"
							title="Link">Current tasks</router-link>
					</li>
					<li>
						<router-link to="/hidden"
							:class="`block px-4 py-1 md:p-2 lg:px-4 ${$route.path === '/hidden' ? $color('navLinkFocused') + '-600' : ''}`"
							title="Link">Hidden</router-link>
					</li>
					<li>
						<router-link to="/archive"
							:class="`block px-4 py-1 md:p-2 lg:px-4 ${$route.path === '/archive' ? $color('navLinkFocused') + '-600': ''}`"
							title="Link">Archive</router-link>
					</li>
					<li>
						<router-link
							to="/projects-categories"
							:class="`block px-4 py-1 md:p-2 lg:px-4 ${$route.path === '/projects-categories' ? $color('navLinkFocused') + '-600' : ''}`"
							title="Link">Categories</router-link>
					</li>
				</ul>
				<span :class="`${$color('navTextUser')}-500`" class="flex justify-between items-center ml-auto mr-4" :key="rerenderSwitcher">
					<day-night-switch v-model="switchOn"/>
				</span>
				<account-dropdown />
			</div>
		</div>
	</nav>
</template>

<script>
	import DayNightSwitch from './DayNightSwitch'
	import AccountDropdown from './AccountDropdown'

	export default {
		name: 'Navbar',
		components: {AccountDropdown, DayNightSwitch},
		computed: {
			switchOn: {
				get () {
					return this.$store.getters.colorScheme === 'dark'
				},
				set (newValue) {
					this.$store.commit('colorScheme', newValue ? 'dark' : 'default')
				}
			}
		},
		data() {
			return {
				isHidden: true,
				rerenderSwitcher: 0
			}
		}
	}
</script>
