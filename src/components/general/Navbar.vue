<template>
	<div class="absolute right-0 z-50 ml-auto p-4 md:hidden">
		<div
			class="nav-toggle-menu"
			:class="{ active: showMenu }"
			@click="toggleMenu"
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
		<div class="container mx-auto flex p-4">
			<navbar-menu @navigated="toggleMenu" />

			<div
				class="ml-auto mr-4 flex items-center justify-between text-black dark:text-white"
			>
				<day-night-switch :key="store.state.colorScheme" v-model="switchOn" />
			</div>

			<account-dropdown />
		</div>
	</nav>

	<transition name="translate-left" mode="out-in">
		<nav
			v-if="showMenu"
			class="fixed inset-0 z-40 w-full bg-white shadow transition-colors duration-300 dark:bg-gray-900 md:hidden"
			role="navigation"
		>
			<div class="mt-4 flex h-full flex-col items-center p-4 text-center">
				<navbar-menu @navigated="toggleMenu" />

				<account-dropdown />

				<div
					class="mt-auto mb-4 flex items-center justify-between text-black dark:text-white"
				>
					<day-night-switch :key="store.state.colorScheme" v-model="switchOn" />
				</div>
			</div>
		</nav>
	</transition>
</template>

<script lang="ts" setup>
	import DayNightSwitch from './DayNightSwitch.vue';
	import AccountDropdown from './AccountDropdown.vue';
	import NavbarMenu from 'src/components/general/NavbarMenu.vue';
	import { computed, ref } from 'vue';
	import store from 'src/store';

	const showMenu = ref(false);

	const switchOn = computed({
		get() {
			return store.state.colorScheme === 'dark';
		},
		set(value) {
			store.commit('setColorScheme', value ? 'dark' : 'default');
		},
	});

	function toggleMenu() {
		showMenu.value = !showMenu.value;

		document.body.classList.toggle('overflow-hidden');
	}
</script>
