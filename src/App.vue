<template>
	<router-view v-if="isQuickAddWindow" />
	<template v-else-if="isDetachedWindow">
		<alert ref="alert" />
		<router-view />
		<PagesImportDialog
			v-if="pagesImport.open"
			:key="pagesImport.key"
			:open="pagesImport.open"
			:files="pagesImport.files"
			:parent-id="pagesImport.parentId"
			:parent-title="pagesImport.parentTitle"
			:workspace-code="pagesImport.workspaceCode"
			@update:open="closePagesImport"
		/>
		<Toaster />
	</template>
	<template v-else>
	<alert ref="alert" />

	<div
		v-if="isDesktop && !$store.getters.isLoggedIn"
		data-tauri-drag-region
		class="fixed inset-x-0 top-0 z-50 flex h-10 items-center px-3"
	>
		<WindowControls />
	</div>

	<div
		class="font-sans text-tmgr-blue dark:text-tmgr-gray"
		:key="$store.state.appRerenderKey"
	>
		<div class="flex min-h-screen">
			<CustomSidebar>
				<router-view v-slot="{ Component, route }">
					<component :is="Component" :key="routeViewKey(route)" />
				</router-view>
			</CustomSidebar>
		</div>

		<StatusBar
			v-if="isDesktop && $store.getters.isLoggedIn"
			:tasks="activeTasks"
		/>
		<ActiveTasks v-else :tasks="activeTasks" />
		<DesktopTray
			v-if="isDesktop && $store.getters.isLoggedIn"
			:tasks="activeTasks"
		/>
		<DesktopHotkeys
			v-if="isDesktop && $store.getters.isLoggedIn"
			:tasks="activeTasks"
		/>
		<DesktopDownloads v-if="isDesktop" />
		<DesktopUpdateCheck v-if="isDesktop" />
		<DesktopAuthLinks v-if="isDesktop && !$store.getters.isLoggedIn" />

		<Transition name="fade">
			<TaskSidePanel
				v-if="showTaskFormModalWindow"
				close-on-bg-click
				@close="handleModalClose"
			>
				<NewForm
					:is-modal="true"
					:key="`${$store.state.currentTaskIdForModal || 'new'}-${
						$store.state.createTaskInProjectCategoryId || 'none'
					}`"
					@close="handleModalClose"
				/>
			</TaskSidePanel>
		</Transition>
	</div>

	<DesktopWhatsNew v-if="isDesktop && isMainWindow" />
	<PagesImportDialog
		v-if="pagesImport.open"
		:key="pagesImport.key"
		:open="pagesImport.open"
		:files="pagesImport.files"
		:parent-id="pagesImport.parentId"
		:parent-title="pagesImport.parentTitle"
		:workspace-code="pagesImport.workspaceCode"
		@update:open="closePagesImport"
	/>
	<Toaster />
	</template>
</template>

<script>
	import { getDailyTasksCount } from '@/actions/tmgr/daily-tasks';
	import { getLaunchedTasks, getTask } from '@/actions/tmgr/tasks';
	import { getUserSettings, updateUserSettingsV2 } from '@/actions/tmgr/user';
	import {
		getWorkspaceStatuses,
		getWorkspaces,
	} from '@/actions/tmgr/workspaces';
	import ActiveTasks from '@/components/ActiveTasks.vue';
	import DesktopAuthLinks from '@/components/desktop/DesktopAuthLinks.vue';
	import DesktopDownloads from '@/components/desktop/DesktopDownloads.vue';
	import DesktopHotkeys from '@/components/desktop/DesktopHotkeys.vue';
	import DesktopTray from '@/components/desktop/DesktopTray.vue';
	import DesktopUpdateCheck from '@/components/desktop/DesktopUpdateCheck.vue';
	import DesktopWhatsNew from '@/components/desktop/DesktopWhatsNew.vue';
	import StatusBar from '@/components/desktop/StatusBar.vue';
	import WindowControls from '@/components/desktop/WindowControls.vue';
	import Alert from '@/components/general/Alert.vue';
	import CustomSidebar from '@/components/general/CustomSidebar.vue';
	import Modal from '@/components/Modal.vue';
	import TaskSidePanel from '@/components/tasks/TaskSidePanel.vue';
	import { Toaster } from '@/components/ui/toast';
	import {
		closePagesImport,
		pagesImportRequest,
	} from '@/composable/usePagesMarkdownIo';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { startRoutineScheduler, stopRoutineScheduler } from '@/local/routines/scheduler';
	import store from '@/store';
	import { desktopWindowLabel, isDesktopApp } from '@/utils/desktop';
	import { syncActiveLocalWorkspace } from '@/utils/localWorkspaceSync';
	import { useRunningTimerSync } from '@/composable/useRunningTimerSync';
	import { isDetachedWindowLabel } from '@/utils/taskWindow';
	import { routeViewKey } from '@/utils/routeViewKey';
	import { generateTaskUrl } from '@/utils/url';
	import {
		defineAsyncComponent,
		defineComponent,
		onBeforeMount,
		onBeforeUnmount,
		ref,
		watch,
	} from 'vue';
	const NewForm = defineAsyncComponent(() => import('@/pages/NewForm.vue'));
	const PagesImportDialog = defineAsyncComponent(
		() => import('@/components/pages/PagesImportDialog.vue'),
	);

	const DEFAULT_TRANSITION = 'fade';

	export default defineComponent({
		name: 'App',
		components: {
			PagesImportDialog,
			Toaster,
			CustomSidebar,
			NewForm,
			ActiveTasks,
			DesktopAuthLinks,
			DesktopDownloads,
			DesktopHotkeys,
			DesktopTray,
			DesktopUpdateCheck,
			DesktopWhatsNew,
			Modal,
			TaskSidePanel,
			Alert,
			WindowControls,
			StatusBar,
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

			const isDetachedWindow = isDetachedWindowLabel(desktopWindowLabel());
			if (!isDetachedWindow && desktopWindowLabel() !== 'quick-add') {
				const reloadActiveTasks = () =>
					store.commit('incrementReloadActiveTasksKey');
				useRunningTimerSync({
					onEvent: reloadActiveTasks,
					onResync: reloadActiveTasks,
				});
			}
			if (!isDetachedWindow) {
				watch(
					() => store.getters.isLoggedIn,
					(loggedIn) => (loggedIn ? startRoutineScheduler() : stopRoutineScheduler()),
					{ immediate: true },
				);
				onBeforeUnmount(stopRoutineScheduler);
			}

			return {
				routeViewKey,
				pagesImport: pagesImportRequest,
				closePagesImport,
				isDesktop: isDesktopApp(),
				isQuickAddWindow: desktopWindowLabel() === 'quick-add',
				isDetachedWindow,
				isMainWindow: desktopWindowLabel() === 'main',
			};
		},
		data() {
			return {
				prevHeight: 0,
				transitionName: DEFAULT_TRANSITION,
				activeTasks: [],
				activeTasksRequest: 0,
				bodyOverflow: '',
				bodyHeight: 800,
			};
		},
		computed: {
			activeTasksContext() {
				return `${this.$store.state.sessionGeneration}:${this.$store.state.user?.id}:${this.$store.getters.currentWorkspaceId}`;
			},
			switchOn: {
				get() {
					return this.$store.getters.isDarkTheme;
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
			activeTasksContext() {
				this.activeTasks = [];
				this.loadActiveTasks();
			},
			'$store.state.reloadActiveTasksKey'() {
				this.loadActiveTasks();
			},
			'$route.params.workspace_code': {
				async handler(workspaceCode) {
					if (this.isDetachedWindow) return;
					if (workspaceCode && this.$store.getters.isLoggedIn) {
						// If URL has workspace code, check if it matches current workspace
						const workspaces = this.$store.state.workspaces;
						if (!workspaces || !workspaces.length) {
							await this.loadWorkspaces();
						}

						const currentWorkspaceId = this.$store.state.user?.settings?.find(
							(setting) => setting.key === 'current_workspace',
						)?.value;

						const currentWorkspace = this.$store.state.workspaces.find(
							(workspace) =>
								Number(workspace.id) === Number(currentWorkspaceId),
						);

						// Find workspace by code from URL
						const workspaceFromUrl = this.$store.state.workspaces.find(
							(workspace) => workspace.code === workspaceCode,
						);

						// If workspace from URL exists and is different from current workspace, switch to it
						if (
							workspaceFromUrl &&
							(!currentWorkspace || currentWorkspace.code !== workspaceCode)
						) {
							console.log(
								`Switching workspace from ${currentWorkspace?.code} to ${workspaceCode}`,
							);
							await this.changeWorkspace(workspaceFromUrl);
						}
					}
				},
				immediate: true,
			},
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
				const request = ++this.activeTasksRequest;
				const userId = this.$store.state.user?.id;
				const workspaceId = this.$store.getters.currentWorkspaceId;
				const generation = this.$store.state.sessionGeneration;
				if (!userId) {
					this.activeTasks = [];
					return;
				}
				const current = () =>
					request === this.activeTasksRequest &&
					userId === this.$store.state.user?.id &&
					workspaceId === this.$store.getters.currentWorkspaceId &&
					generation === this.$store.state.sessionGeneration;
				try {
					const tasks = await getLaunchedTasks();
					if (current()) this.activeTasks = tasks || [];
				} catch (error) {
					console.error('Error loading active tasks:', error);
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
				if (
					!this.$store.state.workspaces ||
					this.$store.state.workspaces.length === 0
				) {
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
					const currentWorkspaceId = Number(
						this.$store.state.user?.settings?.find(
							(setting) => setting.key === 'current_workspace',
						)?.value,
					);

					// Find workspace by ID, converting workspace.id to number for comparison
					const currentWorkspace = workspaces.find(
						(workspace) => Number(workspace.id) === currentWorkspaceId,
					);

					if (!currentWorkspace) {
						console.error(
							'Current workspace not found. Available workspaces:',
							workspaces,
							'Current workspace ID:',
							currentWorkspaceId,
						);
						return;
					}

					// Fetch task data directly from API
					const task = await getTask(taskId);
					const category =
						task?.category && typeof task.category === 'object'
							? task.category
							: null;

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
						(setting) => setting.key === 'current_workspace',
					);

					if (workspaceSetting) {
						let nextWorkspacePath = null;
						const currentPath = window.location.pathname;
						const pathParts = currentPath.split('/');

						// Check if current page is workspace-independent (like /routines, /settings, /profile)
						const workspaceIndependentPages = [
							'routines',
							'settings',
							'profile',
						];
						const isWorkspaceIndependent = workspaceIndependentPages.some(
							(page) => currentPath.includes(`/${page}`),
						);

						if (
							!isWorkspaceIndependent &&
							pathParts.length > 1 &&
							pathParts[1]
						) {
							// Replace the workspace code in the URL for workspace-aware pages only
							pathParts[1] = workspace.code;
							nextWorkspacePath = pathParts.join('/');
						}
						// For workspace-independent pages, don't change the URL at all

						if (workspace.is_local) {
							// local/install.ts activates it and keeps the server default untouched.
							const updatedSettings = settings.map((setting) => ({
								id: setting.id,
								value:
									setting.key === 'current_workspace'
										? workspace.id
										: setting.value,
							}));
							const updatedUser = await updateUserSettingsV2(updatedSettings);
							this.$store.commit('setUser', updatedUser);
						} else {
							await syncActiveLocalWorkspace(workspace.id);
						}

						this.$store.commit('updateUserWorkspaceSetting', {
							workspaceId: workspace.id,
						});

						if (nextWorkspacePath) {
							await this.$router.replace(nextWorkspacePath);
						}

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
					(setting) => setting.key === 'current_workspace',
				)?.value;

				return workspaces.findIndex(
					(workspace) => Number(workspace.id) === Number(currentWorkspaceId),
				);
			},
			handleWorkspaceHotkeys(event) {
				if (!this.$store.getters.isLoggedIn) return;

				const workspaces = this.$store.state.workspaces;
				if (!workspaces || !workspaces.length) return;

				const isCtrlOrCmd = event.ctrlKey || event.metaKey;

				if (
					isCtrlOrCmd &&
					event.altKey &&
					(event.key === 'ArrowRight' || event.key === 'ArrowLeft')
				) {
					event.preventDefault();

					const currentIndex = this.getCurrentWorkspaceIndex();
					if (currentIndex === -1) return;

					let targetIndex;
					if (event.key === 'ArrowRight') {
						targetIndex = (currentIndex + 1) % workspaces.length;
					} else {
						targetIndex =
							currentIndex === 0 ? workspaces.length - 1 : currentIndex - 1;
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
			// user starts as {} (truthy!) — gate on a real loaded user, otherwise
			// guests fire authenticated calls, catch 401s, and the interceptor's
			// hardLogout() yanks the router away from OAuth callback pages
			// mid-exchange (mobile lost that race on every social login).
			if (store.state.user?.id) {
				if (!store.state.workspaces || !store.state.workspaces.length) {
					await this.loadWorkspaces();
				}
				const workspaceFromUrl = store.state.workspaces.find(
					(workspace) => workspace.code === this.$route.params.workspace_code,
				);
				if (workspaceFromUrl) {
					this.$store.commit('updateUserWorkspaceSetting', {
						workspaceId: workspaceFromUrl.id,
					});
					await syncActiveLocalWorkspace(workspaceFromUrl.id);
				}
				await Promise.all([getUserSettings(), getWorkspaceStatuses()]);
			}

			this.$router.beforeEach((to, from, next) => {
				let routeTransitionName =
					to.meta.transitionName || from.meta.transitionName;

				if (routeTransitionName === 'slide') {
					const toDepth = to.path.split('/').length;
					const fromDepth = from.path.split('/').length;
					routeTransitionName =
						toDepth < fromDepth ? 'slide-right' : 'slide-left';
				}

				this.transitionName = routeTransitionName || DEFAULT_TRANSITION;

				if (to.name !== from.name) {
					if (
						to.meta.title &&
						!to.name?.includes('TasksList') &&
						!to.name?.includes('WorkspaceTasksList')
					) {
						this.$store.commit('setMetaTitle', to.meta.title);
						setDocumentTitle(to.meta.title);
					} else if (
						!to.name?.includes('TasksList') &&
						!to.name?.includes('WorkspaceTasksList')
					) {
						this.$store.commit('setMetaTitle', '');
						setDocumentTitle();
					}
				}

				next();
			});

			if (!this.$store.state.user?.id || this.isQuickAddWindow || this.isDetachedWindow) {
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
		},
		mounted() {
			this.initBodyHeight();
			// Guests must not fire authenticated calls: the 401 would trigger the
			// interceptor's hardLogout() and yank the router off a social OAuth
			// callback page before the code exchange completes.
			if (this.$store.getters.isLoggedIn) {
				this.ensureWorkspacesLoaded();
			}
			if (!this.isDetachedWindow) {
				window.addEventListener('keydown', this.handleWorkspaceHotkeys);
			}
		},
		beforeUnmount() {
			window.removeEventListener('keydown', this.handleWorkspaceHotkeys);
		},
	});
</script>

<style lang="scss" src="@/assets/styles/index.scss"></style>
