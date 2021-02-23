<template>
	<nav :class="`${$color('blocks')} shadow`" role="navigation">
		<div class="container mx-auto p-4 flex flex-wrap items-center md:flex-no-wrap">
			<div class="ml-auto md:hidden">
				<button class="flex items-center px-3 py-2 border rounded" type="button" @click="isHidden = !isHidden">
					Menu
				</button>
			</div>
			<div class="w-full md:w-auto md:flex-grow md:flex md:items-center md:block" :class="{ hidden: isHidden }">
				<navbar-menu />

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
	import NavbarMenu from "@/components/UIElements/NavbarMenu";

	export default {
		name: 'Navbar',
		components: {
			NavbarMenu,
			AccountDropdown,
			DayNightSwitch
		},
		data: () => ({
			isHidden: true,
			rerenderSwitcher: 0,
			links: [
				{ id: 1, name: 'Current tasks', path: '/' },
				{ id: 1, name: 'Hidden', path: '/hidden' },
				{ id: 1, name: 'Archive', path: '/acrhive' },
				{ id: 1, name: 'Categories', path: '/projects-categories' },
			]
		}),
		computed: {
			switchOn: {
				get () {
					return this.$store.getters.colorScheme === 'dark'
				},
				set (newValue) {
					this.$store.commit('colorScheme', newValue ? 'dark' : 'default')
				}
			}
		}
	}
</script>
