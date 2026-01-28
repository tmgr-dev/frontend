<template>
	<alert ref="alert" />

	<div
		class="font-sans text-tmgr-blue dark:text-tmgr-gray"
		:key="$store.state.appRerenderKey"
	>
			<div class="flex min-h-screen">
				<!-- CHANGES: Added keep-alive for cached views -->
				<CustomSidebar>
					<router-view v-slot="{ Component, route }">
						<keep-alive :include="keepAliveComponents">
							<component :is="Component" v-if="showComponent" :key="route?.meta?.keepAlive ? route?.name : route?.fullPath" />
						</keep-alive>
					</router-view>
				</CustomSidebar>
			</div>

		<ActiveTasks :tasks="activeTasks" />

		<Transition name="fade">
			<Modal
				v-if="showTaskFormModalWindow"
				modal-class="h-full w-full md:w-auto md:h-auto"
				close-on-bg-click
				@close="handleModalClose"
			>
			<template #modal-body>
				<NewForm 
					:is-modal="true" 
					:key="`${$store.state.currentTaskIdForModal || 'new'}-${$store.state.createTaskInProjectCategoryId || 'none'}`"
					@close="handleModalClose" 
				/>
			</template>
			</Modal>
		</Transition>
	</div>

	<Toaster />
</template>

<script>
	import { defineComponent, defineAsyncComponent, onBeforeMount, ref, watch } from 'vue';
	import TaskForm from '@/pages/TaskForm.vue';
	import store from '@/store';
	import { getUserSettings } from '@/actions/tmgr/user';
	import { getLaunchedTasks, getTask } from '@/actions/tmgr/tasks';
	import { getWorkspaceStatuses, getWorkspaces } from '@/actions/tmgr/workspaces';
	import Alert from '@/components/general/Alert.vue';
	import Modal from '@/components/Modal.vue';
	import ActiveTasks from '@/components/ActiveTasks.vue';
	const NewForm = defineAsyncComponent(() => import('@/pages/NewForm.vue'));
	import CustomSidebar from '@/components/general/CustomSidebar.vue';
	import { getDailyTasksCount } from '@/actions/tmgr/daily-tasks';
	import { Toaster } from '@/components/ui/toast';
	import { generateTaskUrl } from '@/utils/url';
	import { updateUserSettingsV2 } from '@/actions/tmgr/user';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';

	const DEFAULT_TRANSITION = 'fade';

	export default defineComponent({
		name: 'App',
		components: {
			Toaster,
			CustomSidebar,
			NewForm,
			ActiveTasks,
			Modal,
			Alert,
			TaskForm,
		},
		setup() {
			const dailyRoutinesCount = ref(0);
			const isExpanded = ref(true);

			onBeforeMount(async () => {
				if (store.getters.isLoggedIn) {
					dailyRoutinesCount.value = await getDailyTasksCount();
				}
			});

			if (typeof window !== 'undefined') {
				const savedState = localStorage.getItem('sidebarExpanded');
				if (savedState !== null) {
					isExpanded.value = savedState === 'true';
				}

				watch(isExpanded, (newValue) => {
					localStorage.setItem('sidebarExpanded', newValue.toString());
				});
			}
		},
		data() {
			return {
				prevHeight: 0,
				transitionName: DEFAULT_TRANSITION,
				showComponent: true,
				activeTasks: [],
				bodyOverflow: '',
				bodyHeight: 800,
			};
		},
		computed: {
			// CHANGES: Added keepAliveComponents for route caching
			keepAliveComponents() {
				return ['DashboardPage', 'TasksListPage'];
			},
			switchOn: {
				get() {
					return this.$store.state.colorScheme === 'dark';
				},
				set(newValue) {
					this.$store.commit('setColorScheme', newValue ? 'dark' : 'default');
				},
			},
			showTaskFormModalWindow() {
				const taskId = this.$store.state.currentTaskIdForModal;
				
				// Update browser URL when viewing a task in modal
				if (taskId && this.$route.name !== 'TasksEdit') {
					// Update URL asynchronously
					this.updateTaskModalUrl(taskId);
				}
				
				return (
					this.$route.name !== 'TasksEdit' &&
					(this.$store.state.currentTaskIdForModal ||
						this.$store.state.showCreatingTaskModal)
				);
			},
		},
		watch: {
			'$route.path'() {
				this.showComponent = false;
				setTimeout(() => (this.showComponent = true), 100);
			},
			'$route.name'(to, from) {
				if (to !== from) {
					if (this.$route.meta.title) {
						this.$store.commit('setMetaTitle', this.$route.meta.title);
						setDocumentTitle(this.$route.meta.title);
					} else {
						this.$store.commit('setMetaTitle', '');
						setDocumentTitle();
					}
				}
			},
			'$store.state.reloadActiveTasksKey'() {
				this.loadActiveTasks();
			},
			'$route.params.workspace_code': {
				async handler(workspaceCode) {
					if (workspaceCode && this.$store.getters.isLoggedIn) {
						// If URL has workspace code, check if it matches current workspace
						const workspaces = this.$store.state.workspaces;
						if (!workspaces || !workspaces.length) {
							await this.loadWorkspaces();
						}
						
						const currentWorkspaceId = this.$store.state.user?.settings?.find(
							setting => setting.key === 'current_workspace'
						)?.value;
						
						const currentWorkspace = this.$store.state.workspaces.find(
							workspace => Number(workspace.id) === Number(currentWorkspaceId)
						);
						
						// Find workspace by code from URL
						const workspaceFromUrl = this.$store.state.workspaces.find(
							workspace => workspace.code === workspaceCode
						);
						
						// If workspace from URL exists and is different from current workspace, switch to it
						if (workspaceFromUrl && (!currentWorkspace || currentWorkspace.code !== workspaceCode)) {
							console.log(`Switching workspace from ${currentWorkspace?.code} to ${workspaceCode}`);
							await this.changeWorkspace(workspaceFromUrl);
						}
					}
				},
				immediate: true
			}
		},
		methods: {
			beforeLeave(element) {
				this.prevHeight = getComputedStyle(element).height;
			},
			enter(element) {
				const { height } = getComputedStyle(element);

				element.style.height = this.prevHeight;

				setTimeout(() => {
					element.style.height = height;
				});
			},
			afterEnter(element) {
				element.style.height = 'auto';
			},
			async loadActiveTasks() {
				try {
					if (!this.$store.state.user) return;
					
					const tasks = await getLaunchedTasks();
					this.activeTasks = tasks || [];
				} catch (error) {
					console.error('Error loading active tasks:', error);
					this.activeTasks = [];
				}
			},
			minimize() {
				if (import.meta.env.MODE === 'electron') {
					window.myWindowAPI.minimize();
				}
			},
			maximize() {
				if (import.meta.env.MODE === 'electron') {
					window.myWindowAPI.toggleMaximize();
				}
			},
			closeApp() {
				if (import.meta.env.MODE === 'electron') {
					window.myWindowAPI.close();
				}
			},
			handleNewTask() {
				this.$store.commit('setShowCreatingTaskModal');
			},
			initBodyHeight() {
				setTimeout(() => {
					try {
						this.bodyHeight = this.getBodyHeight();
					} catch (e) {
						setTimeout(() => (this.bodyHeight = this.getBodyHeight()), 1000);
					}
				}, 500);
			},
			getBodyHeight() {
				return (
					this.getOffsetHeightOfElement('body') +
					30 -
					this.getOffsetHeightOfElement('[role=toolbar]') -
					this.getOffsetHeightOfElement('nav')
				);
			},
			getOffsetHeightOfElement(selector) {
				const el = document.querySelector(selector);
				if (!el) {
					return 0;
				}
				return el.offsetHeight;
			},
			ensureWorkspacesLoaded() {
				if (!this.$store.state.workspaces || this.$store.state.workspaces.length === 0) {
					this.$store.dispatch('loadWorkspaces');
				}
			},
			async updateTaskModalUrl(taskId) {
				try {
					// Ensure workspaces are loaded
					let workspaces = this.$store.state.workspaces;
					if (!workspaces || !workspaces.length) {
						workspaces = await getWorkspaces();
						this.$store.commit('setWorkspaces', workspaces);
					}
					
					// Get current workspace ID and ensure it's a number
					const currentWorkspaceId = Number(this.$store.state.user?.settings?.find(
						setting => setting.key === 'current_workspace'
					)?.value);
					
					// Find workspace by ID, converting workspace.id to number for comparison
					const currentWorkspace = workspaces.find(
						workspace => Number(workspace.id) === currentWorkspaceId
					);
					
					if (!currentWorkspace) {
						console.error('Current workspace not found. Available workspaces:', workspaces, 'Current workspace ID:', currentWorkspaceId);
						return;
					}
					
					// Fetch task data directly from API
					const task = await getTask(taskId);
					const category = task?.category && typeof task.category === 'object' ? task.category : null;
					
					// Generate the proper URL with workspace code
					const newPath = generateTaskUrl(taskId, currentWorkspace, category);
					const currentPath = window.location.pathname;
					
					// Only update if the URL isn't already set to this task and the URL is valid
					if (newPath && newPath !== '/' && currentPath !== newPath) {
						// Use replaceState instead of pushState to avoid adding new history entries
						history.replaceState({}, '', newPath);
						this.$store.state.urlManuallyChanged = true;
					}
				} catch (error) {
					console.error('Error updating task modal URL:', error);
				}
			},
			handleModalClose() {
				// Restore original URL if it was changed for the modal
				if (this.$store.state.urlManuallyChanged) {
					// Get the URL without the task ID
					const currentPath = window.location.pathname;
					const pathParts = currentPath.split('/');
					
					// If we have a numeric task ID at the end, remove it
					if (/^\d+$/.test(pathParts[pathParts.length - 1])) {
						pathParts.pop();
						const newPath = pathParts.join('/') || '/';
						history.replaceState({}, '', newPath);
					}
				}
				
				// Close the modal
				this.$store.commit('closeTaskModal');
			},
			async loadWorkspaces() {
				try {
					const workspaces = await getWorkspaces();
					this.$store.commit('setWorkspaces', workspaces);
					return workspaces;
				} catch (error) {
					console.error('Error loading workspaces:', error);
					return [];
				}
			},
			async changeWorkspace(workspace) {
				try {
					// Get current user settings
					const settings = this.$store.state.user?.settings || [];
					
					// Find the current workspace setting
					const workspaceSetting = settings.find(
						setting => setting.key === 'current_workspace'
					);
					
					if (workspaceSetting) {
						// Update the URL to match the new workspace code
						const currentPath = window.location.pathname;
						const pathParts = currentPath.split('/');
						
						// Check if current page is workspace-independent (like /routines, /settings, /profile)
						const workspaceIndependentPages = ['routines', 'settings', 'profile'];
						const isWorkspaceIndependent = workspaceIndependentPages.some(
							page => currentPath.includes(`/${page}`)
						);
						
						if (!isWorkspaceIndependent && pathParts.length > 1 && pathParts[1]) {
							// Replace the workspace code in the URL for workspace-aware pages only
							pathParts[1] = workspace.code;
							const newPath = pathParts.join('/');
							
							// Use history.replaceState to update the URL without navigating
							history.replaceState({}, '', newPath);
						}
						// For workspace-independent pages, don't change the URL at all
						
						// Prepare updated settings
						const updatedSettings = settings.map(setting => {
							if (setting.key === 'current_workspace') {
								return {
									id: setting.id,
									value: workspace.id
								};
							}
							return {
								id: setting.id,
								value: setting.value
							};
						});
						
						// Update user settings in the backend
						await updateUserSettingsV2(updatedSettings);
						
						// Create a new user object with updated settings to ensure reactivity
						const updatedUser = {
							...this.$store.state.user,
							settings: updatedSettings
						};
						
						// Update store without reloading the page
						this.$store.commit('setUser', updatedUser);
						
						// Also update the workspace setting directly for components watching that specifically
						this.$store.commit('updateUserWorkspaceSetting', {
							workspaceId: workspace.id
						});
						
						// Force reload active tasks with new workspace context
						await this.loadActiveTasks();
						
						// Update the meta title if needed
						if (this.$route.meta.title) {
							this.$store.commit('setMetaTitle', this.$route.meta.title);
							setDocumentTitle(this.$route.meta.title);
						}
						
						// Force UI components to update by triggering an app rerender
						this.$store.commit('rerenderApp');
						
						console.log('Workspace successfully changed to:', workspace.name);
					}
				} catch (error) {
					console.error('Error changing workspace:', error);
				}
			},
			getCurrentWorkspaceIndex() {
				const workspaces = this.$store.state.workspaces;
				if (!workspaces || !workspaces.length) return -1;
				
				const currentWorkspaceId = this.$store.state.user?.settings?.find(
					setting => setting.key === 'current_workspace'
				)?.value;
				
				return workspaces.findIndex(
					workspace => Number(workspace.id) === Number(currentWorkspaceId)
				);
			},
			handleWorkspaceHotkeys(event) {
				if (!this.$store.getters.isLoggedIn) return;
				
				const workspaces = this.$store.state.workspaces;
				if (!workspaces || !workspaces.length) return;
				
				const isCtrlOrCmd = event.ctrlKey || event.metaKey;
				
				if (isCtrlOrCmd && event.altKey && (event.key === 'ArrowRight' || event.key === 'ArrowLeft')) {
					event.preventDefault();
					
					const currentIndex = this.getCurrentWorkspaceIndex();
					if (currentIndex === -1) return;
					
					let targetIndex;
					if (event.key === 'ArrowRight') {
						targetIndex = (currentIndex + 1) % workspaces.length;
					} else {
						targetIndex = currentIndex === 0 ? workspaces.length - 1 : currentIndex - 1;
					}
					
					this.changeWorkspace(workspaces[targetIndex]);
				} else if (isCtrlOrCmd && /^[1-9]$/.test(event.key)) {
					event.preventDefault();
					
					const workspaceIndex = parseInt(event.key) - 1;
					if (workspaceIndex < workspaces.length) {
						this.changeWorkspace(workspaces[workspaceIndex]);
					}
				}
			},
		},
		async created() {
			if (store.state.user) {
				await Promise.all([
					getUserSettings(), 
					getWorkspaceStatuses(),
					this.$store.dispatch('loadWorkspaces')
				]);
				await this.loadActiveTasks();
			}

			this.$router.beforeEach((to, from, next) => {
				this.loadActiveTasks();
				let routeTransitionName = to.meta.transitionName || from.meta.transitionName;

				if (routeTransitionName === 'slide') {
					const toDepth = to.path.split('/').length;
					const fromDepth = from.path.split('/').length;
					routeTransitionName = toDepth < fromDepth ? 'slide-right' : 'slide-left';
				}

				this.transitionName = routeTransitionName || DEFAULT_TRANSITION;

				if (to.name !== from.name) {
					if (to.meta.title && !to.name?.includes('TasksList') && !to.name?.includes('WorkspaceTasksList')) {
						this.$store.commit('setMetaTitle', to.meta.title);
						setDocumentTitle(to.meta.title);
					} else if (!to.name?.includes('TasksList') && !to.name?.includes('WorkspaceTasksList')) {
						this.$store.commit('setMetaTitle', '');
						setDocumentTitle();
					}
				}

				next();
			});

			if (!this.$store.state.user) {
				return;
			}
			this.$store.getters.getPusherBeamsClient.getUserId().then((userId) => {
				if (!userId) {
					return this.$store.commit('setPusherBeamsUserId', userId);
				}
				userId = this.$store.state.user.id.toString();
				this.$store.getters.getPusherBeamsClient.start().then(() => {
					this.$store.getters.getPusherBeamsClient
						.setUserId(userId, this.$store.getters.getPusherTokenProvider)
						.then(() => {
							this.$store.commit('setPusherBeamsUserId', userId);
						});
				});
			});
			if (import.meta.env.MODE === 'electron') {
				document.body.style.overflow = 'hidden';
			}
		},
		mounted() {
			this.initBodyHeight();
			window.addEventListener('keydown', this.handleWorkspaceHotkeys);
		},
		beforeUnmount() {
			window.removeEventListener('keydown', this.handleWorkspaceHotkeys);
		},
	});
</script>

<style lang="scss" src="@/assets/styles/index.scss"></style>
