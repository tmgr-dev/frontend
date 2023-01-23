<template>
	<div class="ml-auto md:hidden absolute z-30 right-0 p-4">
		<div
			class="nav-toggle-menu"
			:class="{ active: menuIsActive }"
			@click="$store.getters.slideout.toggle()"
		>
			<div class="bar1 bg-gray-900 dark:bg-white"></div>
			<div class="bar2 bg-gray-900 dark:bg-white"></div>
			<div class="bar3 bg-gray-900 dark:bg-white"></div>
		</div>
	</div>

	<nav
		class="transition-colors duration-300 shadow md:block hidden"
		role="navigation"
	>
		<div
			class="container mx-auto p-4 flex flex-wrap items-center md:flex-no-wrap"
		>
			<div class="w-full md:w-auto md:flex-grow md:flex md:items-center hidden">
				<navbar-menu />
				<span
					class="flex justify-between items-center ml-auto mr-4 text-black dark:text-white"
				>
					<day-night-switch />
				</span>

				<account-dropdown />
			</div>
		</div>
	</nav>
</template>

<script>
	import DayNightSwitch from './DayNightSwitch';
	import AccountDropdown from './AccountDropdown';
	import NavbarMenu from 'src/components/UIElements/NavbarMenu';

	export default {
		name: 'Navbar',
		props: {
			menuIsActive: {
				type: Boolean,
				required: false,
				default: false,
			},
			menuPosition: {
				type: Number,
				required: false,
				default: 0,
			},
		},
		components: {
			NavbarMenu,
			AccountDropdown,
			DayNightSwitch,
		},
	};
</script>
