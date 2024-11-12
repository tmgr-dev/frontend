<template>
	<div class="flex min-h-screen">
		<!-- Sidebar -->
		<aside
			v-if="$store.getters.isLoggedIn"
			class="fixed left-0 top-0 h-full transition-all duration-300"
			:class="[isExpanded ? 'w-64' : 'w-16', 'bg-gray-900 text-white']"
		>
			<div class="relative h-full p-4">
				<!-- Toggle Button -->
				<button
					v-if="isExpanded"
					class="absolute right-3 top-6 z-10 flex h-6 w-6 items-center justify-center rounded-full bg-gray-900 text-white hover:bg-gray-800"
					@click="toggleSidebar"
				>
					<span class="material-icons text-sm">
						{{ isExpanded ? 'chevron_left' : 'chevron_right' }}
					</span>
				</button>

				<div class="mb-8 flex items-center">
					<div class="flex items-center">
						<img class="h-auto w-12" src="./assets/img/simple-logo-light.svg" />
					</div>
				</div>

				<nav class="space-y-2">
					<button
						v-if="!isExpanded"
						class="flex items-center rounded-lg px-2 py-2 text-gray-300 hover:bg-gray-800"
						@click="toggleSidebar"
					>
						<span class="material-icons text-lg"> chevron_right </span>
					</button>
					<router-link
						v-for="item in menuItems"
						:key="item.id"
						:to="item.path"
						class="flex items-center rounded-lg px-4 py-2 text-gray-300 hover:bg-gray-800"
						:class="{
							'bg-gray-800 text-white': isCurrentRoute(item.path),
							'px-4': isExpanded,
							'px-2': !isExpanded,
						}"
					>
						<span
							class="material-icons text-lg"
							:class="{ 'mr-3': isExpanded }"
						>
							{{ item.icon }}
						</span>
						<span :class="{ hidden: !isExpanded }">{{ item.name }}</span>
						<span
							v-if="item.count && isExpanded"
							class="ml-auto rounded-full bg-gray-800 px-2 py-1 text-xs"
						>
							{{ item.count }}
						</span>
					</router-link>
				</nav>

				<div class="mt-8">
					<h3
						class="mb-4 text-xs uppercase text-gray-500"
						:class="{ 'text-center': !isExpanded }"
					>
						{{ isExpanded ? 'TEAM MANAGE' : 'TEAM' }}
					</h3>
					<nav class="space-y-2">
						<router-link
							v-for="item in teamItems"
							:key="item.id"
							:to="item.path"
							class="flex items-center rounded-lg px-4 py-2 text-gray-300 hover:bg-gray-800"
							:class="{
								'bg-gray-800 text-white': isCurrentRoute(item.path),
								'px-4': isExpanded,
								'px-2': !isExpanded,
							}"
						>
							<span
								class="material-icons text-lg"
								:class="{ 'mr-3': isExpanded }"
							>
								{{ item.icon }}
							</span>
							<span :class="{ hidden: !isExpanded }">{{ item.name }}</span>
						</router-link>
						<div class="mt-20">
							<day-night-switch
								:style="isExpanded ? 'zoom: 75%;' : 'zoom: 50%;'"
								:key="store.state.colorScheme"
								v-model="switchOn"
							/>
						</div>
						<div class="mt-20">
							<account-dropdown class="md:hidden" />
						</div>
					</nav>
				</div>
			</div>
		</aside>

		<!-- Main Content Area -->
		<div
			class="w-full flex-1 transition-all duration-300"
			:class="{
				'pl-64': $store.getters.isLoggedIn && isExpanded,
				'pl-16': $store.getters.isLoggedIn && !isExpanded,
			}"
		>
			<!-- Top Navigation Bar -->
			<header
				v-if="$store.getters.isLoggedIn"
				class="sticky top-0 z-10 bg-white shadow dark:bg-gray-900"
			>
				<div class="flex items-center justify-between px-4 py-4">
					<div class="flex items-center">
						<div
							class="relative w-48 md:w-60 lg:w-96"
							@click="showAlert = true"
						>
							<span
								class="material-icons absolute left-3 top-2.5 text-gray-400"
							>
								search
							</span>
							<input
								type="text"
								disabled
								placeholder="Search folders, tasks"
								class="w-full rounded-lg border border-gray-300 bg-gray-50 py-2 pl-10 pr-4 focus:border-blue-500 focus:outline-none dark:border-gray-700 dark:bg-gray-800"
							/>
						</div>

						<div class="ml-4 hidden md:block">
							<WorkspaceSelect />
						</div>
					</div>

					<div class="flex items-center space-x-4">
						<account-dropdown class="hidden md:block" />
					</div>
				</div>
			</header>

			<!-- Page Content -->
			<main class="">
				<slot></slot>
			</main>
			<Transition name="fade">
				<confirm
					v-if="showAlert"
					body="Search under development"
					title="Feature under development"
					@onCancel="showAlert = false"
					@onOk="showAlert = false"
				/>
			</Transition>
		</div>
	</div>
</template>

<script setup>
	import { computed, onBeforeMount, ref, watch } from 'vue';
	import { useRoute } from 'vue-router';
	import store from '@/store';
	import DayNightSwitch from '@/components/general/DayNightSwitch.vue';
	import WorkspaceSelect from '@/components/general/WorkspaceSelect.vue';
	import { getDailyTasksCount } from '@/actions/tmgr/daily-tasks';
	import AccountDropdown from '@/components/general/AccountDropdown.vue';
	import Confirm from '@/components/general/Confirm.vue';

	const route = useRoute();
	const dailyRoutinesCount = ref(0);
	const showAlert = ref(false);
	const isDarkMode = ref(false);
	const isExpanded = ref(true);

	const menuItems = [
		{
			id: 1,
			name: 'List',
			path: '/',
			icon: 'list',
		},
		{
			id: 1,
			name: 'Dashboard',
			path: '/board',
			icon: 'dashboard',
		},
		{
			id: 2,
			name: 'My folders',
			path: '/projects-categories',
			icon: 'folder',
		},
		{
			id: 3,
			name: 'Daily routines',
			path: '/daily-routines',
			icon: 'schedule',
			count: dailyRoutinesCount,
		},
	];

	const teamItems = [
		{
			id: 2,
			name: 'Archive',
			path: '/archive',
			icon: 'inventory',
		},
		{
			id: 3,
			name: 'Settings',
			path: '/settings',
			icon: 'settings',
		},
	];

	onBeforeMount(async () => {
		if (store.getters.isLoggedIn) {
			dailyRoutinesCount.value = await getDailyTasksCount();
		}
	});

	const switchOn = computed({
		get() {
			return store.state.colorScheme === 'dark';
		},
		set(value) {
			isDarkMode.value = value ? 'dark' : 'default';
			store.commit('setColorScheme', isDarkMode.value);
		},
	});

	const isCurrentRoute = (path) => {
		return route.path === path;
	};

	const toggleDarkMode = () => {
		isDarkMode.value = !isDarkMode.value;
		document.documentElement.classList.toggle('dark');
	};

	const toggleSidebar = () => {
		isExpanded.value = !isExpanded.value;
	};

	// Optional: Save sidebar state to localStorage
	if (typeof window !== 'undefined') {
		const savedState = localStorage.getItem('sidebarExpanded');
		if (savedState !== null) {
			isExpanded.value = savedState === 'true';
		}

		watch(isExpanded, (newValue) => {
			localStorage.setItem('sidebarExpanded', newValue.toString());
		});
	}
</script>

<style>
	/* For handling dark mode transitions */
	.dark {
		color-scheme: dark;
	}

	@keyframes fadeIn {
		from {
			opacity: 0;
		}
		to {
			opacity: 1;
		}
	}

	.fade-enter-active,
	.fade-leave-active {
		transition: opacity 0.2s ease;
	}

	.fade-enter-from,
	.fade-leave-to {
		opacity: 0;
	}

	/* Optional: Add hover tooltips for collapsed sidebar */
	@media (max-width: 768px) {
		.tooltip {
			position: relative;
		}

		.tooltip:hover::after {
			content: attr(data-tooltip);
			position: absolute;
			left: 100%;
			top: 50%;
			transform: translateY(-50%);
			background: rgba(0, 0, 0, 0.8);
			color: white;
			padding: 4px 8px;
			border-radius: 4px;
			font-size: 12px;
			white-space: nowrap;
			z-index: 1000;
			margin-left: 8px;
		}
	}
</style>
