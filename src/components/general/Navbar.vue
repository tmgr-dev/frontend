<template>
	<div class="absolute right-0 z-50 ml-auto p-4 md:hidden">
		<div
			class="nav-toggle-menu"
			:class="{ active: showMenu }"
			@click="toggleMenu"
		>
			<div class="bar1 bg-ink"></div>
			<div class="bar2 bg-ink"></div>
			<div class="bar3 bg-ink"></div>
		</div>
	</div>

	<nav
		class="hidden bg-surface shadow transition-colors duration-300 md:block"
		role="navigation"
	>
		<div class="container mx-auto flex p-4">
			<navbar-menu @navigated="toggleMenu" />

			<div class="mr-4 ml-auto flex items-center justify-between text-ink">
				<day-night-switch :key="store.state.colorScheme" v-model="switchOn" />
			</div>

			<account-dropdown />
		</div>
	</nav>

	<transition name="translate-left" mode="out-in">
		<nav
			v-if="showMenu"
			class="fixed inset-0 z-40 w-full bg-surface shadow transition-colors duration-300 md:hidden"
			role="navigation"
		>
			<div class="mt-4 flex h-full flex-col items-center p-4 text-center">
				<navbar-menu @navigated="toggleMenu" />

				<account-dropdown />

				<div class="mt-auto mb-4 flex items-center justify-between text-ink">
					<day-night-switch :key="store.state.colorScheme" v-model="switchOn" />
				</div>
			</div>
		</nav>
	</transition>
</template>

<script lang="ts" setup>
	import NavbarMenu from '@/components/general/NavbarMenu.vue';
	import store from '@/store';
	import { computed, ref } from 'vue';
	import AccountDropdown from './AccountDropdown.vue';
	import DayNightSwitch from './DayNightSwitch.vue';

	const showMenu = ref(false);

	const switchOn = computed({
		get() {
			return store.getters.isDarkTheme;
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
