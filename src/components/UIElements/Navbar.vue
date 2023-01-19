<template>
	<div class="ml-auto md:hidden absolute z-30 right-0 p-4">
		<div
			class="nav-toggle-menu"
			:class="{ active: menuIsActive }"
			@click="$store.getters.slideout.toggle()"
		>
			<div class="bar1" :class="$color('burgerIcon')"></div>
			<div class="bar2" :class="$color('burgerIcon')"></div>
			<div class="bar3" :class="$color('burgerIcon')"></div>
		</div>
	</div>

	<nav
		:class="`${$color('menuBg')} shadow md:block tc-hidden`"
		role="navigation"
	>
		<div
			class="container mx-auto p-4 flex flex-wrap items-center md:flex-no-wrap"
		>
			<div
				:class="`w-full md:w-auto md:flex-grow md:flex md:items-center ${
					isHidden ? 'tc-hidden' : ''
				}`"
			>
				<navbar-menu />
				<span
					:class="`${$color('inverseTextColor')}-500`"
					class="flex justify-between items-center ml-auto mr-4"
				>
					<day-night-switch :key="this.$store.getters.colorScheme" v-model="switchOn" />
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
		data: () => ({
			isHidden: true,
			links: [
				{ id: 1, name: 'List', path: '/' },
				{ id: 2, name: 'Archive', path: '/acrhive' },
				{ id: 3, name: 'Categories', path: '/projects-categories' },
			]
		}),
		computed: {
			switchOn: {
				get() {
					return this.$store.getters.colorScheme === 'dark';
				},
				set(newValue) {
					this.$store.commit('colorScheme', newValue ? 'dark' : 'default');
					this.$store.dispatch(
						'putUserSettings',
						this.$store.getters.getUserSettings,
					);
				},
			}
		}
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
