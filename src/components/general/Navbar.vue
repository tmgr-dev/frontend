<template>
	<div class="absolute right-0 z-30 ml-auto p-4 md:hidden">
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
		class="hidden bg-white shadow transition-colors duration-300 dark:bg-gray-900 md:block"
		role="navigation"
	>
		<div
			class="md:flex-no-wrap container mx-auto flex flex-wrap items-center p-4"
		>
			<div
				class="w-full md:flex md:w-auto md:flex-grow md:items-center"
				:class="{ hidden: isHidden }"
			>
				<navbar-menu />
				<span
					class="ml-auto mr-4 flex items-center justify-between text-black dark:text-white"
				>
					<day-night-switch
						:key="this.$store.getters.colorScheme"
						v-model="switchOn"
					/>
				</span>

				<account-dropdown />
			</div>
		</div>
	</nav>
</template>

<script>
	import DayNightSwitch from './DayNightSwitch.vue';
	import AccountDropdown from './AccountDropdown.vue';
	import NavbarMenu from 'src/components/general/NavbarMenu.vue';

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
		data: () => ({
			isHidden: true,
			links: [
				{ id: 1, name: 'List', path: '/' },
				{ id: 2, name: 'Archive', path: '/acrhive' },
				{ id: 3, name: 'Categories', path: '/projects-categories' },
			],
		}),
		computed: {
			switchOn: {
				get() {
					return this.$store.getters.colorScheme === 'dark';
				},
				set(newValue) {
					this.$store.commit('colorScheme', newValue ? 'dark' : 'default');
				},
			},
		},
	};
</script>

<style>
	.fade-kiosk-list-enter-active,
	.fade-kiosk-list-leave-active {
		transition: all 0.9s ease-in-out;
	}

	.mobile-navbar-menu {
		/*transform: translateX(-50%);*/
	}

	.fade-kiosk-list-enter {
		transform: translateX(100%);
	}

	.fade-kiosk-list-leave-to {
		transform: translateX(-100%);
	}
</style>
