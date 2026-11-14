<script setup lang="ts">
	import { getUserFeatureToggles } from '@/actions/tmgr/featureToggles';
	import AsyncContent from '@/components/async/AsyncContent.vue';

	import WindowControls from '@/components/desktop/WindowControls.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import {
		hasNativeTitleBar,
		desktopPlatform,
		isDesktopApp,
	} from '@/utils/desktop';

	import { logout as logoutAction } from '@/actions/tmgr/auth.ts';
	import {
		getUser,
		getUserSettings,
		updateUserSettingsV2,
		User,
	} from '@/actions/tmgr/user.ts';
	import {
		exitWorkspace,
		getWorkspaces,
		Workspace,
	} from '@/actions/tmgr/workspaces.ts';
	import SidebarCategoriesSection from '@/components/categories/SidebarCategoriesSection.vue';
	import ActiveCursorAgents from '@/components/cursor/ActiveCursorAgents.vue';
	import DesktopAppBar from '@/components/desktop/DesktopAppBar.vue';
	import LocalMoveBanner from '@/components/desktop/LocalMoveBanner.vue';
	import LocalOnlyBanner from '@/components/desktop/LocalOnlyBanner.vue';
	import LocalUpgradeHost from '@/components/desktop/LocalUpgradeHost.vue';
	import MoveToCloudHost from '@/components/desktop/MoveToCloudHost.vue';
	import Confirm from '@/components/general/Confirm.vue';
	import DarkMode from '@/components/general/DarkMode.vue';
	import EmailVerifyBanner from '@/components/general/EmailVerifyBanner.vue';
	import GlobalSearch from '@/components/general/GlobalSearch.vue';
	import ModulesPicker from '@/components/general/ModulesPicker.vue';
	import NotificationBell from '@/components/notifications/NotificationBell.vue';
	import SidebarPagesSection from '@/components/pagesNav/SidebarPagesSection.vue';
	import {
		Breadcrumb,
		BreadcrumbItem,
		BreadcrumbLink,
		BreadcrumbList,
		BreadcrumbSeparator,
	} from '@/components/ui/breadcrumb';
	import { Button } from '@/components/ui/button';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuGroup,
		DropdownMenuItem,
		DropdownMenuLabel,
		DropdownMenuSeparator,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { Separator } from '@/components/ui/separator';
	import {
		Sidebar,
		SidebarContent,
		SidebarFooter,
		SidebarGroup,
		SidebarGroupLabel,
		SidebarHeader,
		SidebarInset,
		SidebarMenu,
		SidebarMenuBadge,
		SidebarMenuButton,
		SidebarMenuItem,
		SidebarProvider,
		SidebarRail,
		SidebarTrigger,
	} from '@/components/ui/sidebar';
	import AddTaskModalTrigger from '@/components/ui/sidebar/AddTaskModalTrigger.vue';
	import SidebarMobileCloser from '@/components/ui/sidebar/SidebarMobileCloser.vue';
	import { useDemoWorkspace } from '@/composable/useDemoWorkspace';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import { useNavHistory } from '@/composable/useNavHistory';
	import { isLocalWorkspaceId } from '@/local/classify';
	import { openMoveDialog } from '@/local/moveToCloud/ui';
	import { pluginState } from '@/pluginSystem/state';
	import { t } from '@/i18n';
	import store from '@/store';
	import HelpMenu from '@/tours/ui/HelpMenu.vue';
	import { useTours } from '@/tours/useTours';
	import { syncActiveLocalWorkspace } from '@/utils/localWorkspaceSync';
	import { pagesAvailable } from '@/utils/pagesTree';
	import { requestCache } from '@/utils/requestCache';
	import { serverMessageFrom } from '@/utils/serverError';
	import { workspaceIconStyle } from '@/utils/workspaceColor';
	import { useMediaQuery } from '@vueuse/core';
	import {
		ArchiveIcon,
		ArrowLeft,
		ArrowRight,
		BadgeCheck,
		BarChart3,
		Brain,
		ChevronsUpDown,
		ClipboardListIcon,
		CloudUpload,
		FileDown,
		FolderOpen,
		Gauge,
		HardDrive,
		Inbox,
		LayoutDashboard,
		LifeBuoy,
		LogOut,
		Package,
		PackageOpen,
		PaperclipIcon,
		Plug,
		Plus,
		Settings2,
		Sparkles,
		SquareKanban,
		UserPlus,
		Waypoints,
	} from 'lucide-vue-next';
	import {
		computed,
		defineAsyncComponent,
		onBeforeMount,
		onBeforeUnmount,
		ref,
		watch,
	} from 'vue';
	import { useRoute, useRouter } from 'vue-router';
	import {
		BADGE_DOT_CLASS,
		BADGE_TONE_CLASS,
		viewBadgeAriaLabel,
		viewBadgeLabel,
	} from './sidebarViewBadge';
	import { canLeaveWorkspace as canLeaveWorkspaceFor } from './workspaceLeave';

	const AiAssistantPanel = defineAsyncComponent(
		() => import('@/components/agent/AiAssistantPanel.vue'),
	);

	const LocalAgentPanel = defineAsyncComponent(
		() => import('@/components/agent/LocalAgentPanel.vue'),
	);

	const route = useRoute();
	const router = useRouter();
	const { isFeatureEnabled, showSurface } = useFeatureToggles();

	const { onboarding } = useTours();
	const hubHidden = computed(() => onboarding.value?.hub_hidden === true);

	const isDesktop = isDesktopApp();
	const navHistory = useNavHistory();
	const { canBack, canForward } = navHistory;
	const isLocalOnly = computed(() => store.getters.isLocalOnly === true);
	const { isDemo } = useDemoWorkspace();
	const isMobile = useMediaQuery('(max-width: 768px)');
	const isLocalWorkspace = computed(() =>
		isLocalWorkspaceId(store.getters.currentWorkspaceId),
	);
	const showAskAi = computed(
		() => !isDesktop && !isLocalOnly.value && isFeatureEnabled('ai.assistant'),
	);
	const NAV_ACTIVE =
		'[&.router-link-active]:bg-sidebar-primary/10 [&.router-link-active]:font-medium dark:[&.router-link-active]:bg-sidebar-primary/15 [&.router-link-active>svg]:text-sidebar-primary';
	const showWindowControls = isDesktop && !hasNativeTitleBar();
	const revealLabel = computed(() => {
		const platform = desktopPlatform();
		if (platform === 'windows') return t('nav.sidebar.revealWindows');
		if (platform === 'linux') return t('nav.sidebar.revealLinux');
		return t('nav.sidebar.revealMac');
	});
	const pagesEnabled = computed(() =>
		pagesAvailable(activeWorkspace.value, isFeatureEnabled('pages')),
	);
	const pluginPages = computed(() =>
		Object.values(pluginState.plugins)
			.filter((plugin) => plugin.status === 'running')
			.flatMap((plugin) =>
				plugin.manifest.contributes.views.map((view) => ({
					key: `${plugin.manifest.id}/${view.id}`,
					title: view.title,
					badge: pluginState.viewBadges[`${plugin.manifest.id}:${view.id}`],
				})),
			),
	);

	const openWorkspaceHome = (event: MouseEvent) => {
		const target = event.currentTarget as HTMLElement | null;
		if (event.button !== 0 || target?.closest('[data-collapsible="icon"]')) {
			return;
		}
		event.stopPropagation();
		event.preventDefault();
		router.push('/app');
	};

	const metaTitle = computed(() => {
		return store.state.metaTitle || '';
	});

	watch(
		() => {
			try {
				return route?.meta?.title;
			} catch {
				return null;
			}
		},
		(newTitle) => {
			if (newTitle && !store.state.metaTitle) {
				store.commit('setMetaTitle', t(newTitle as string));
				setDocumentTitle(t(newTitle as string));
			}
		},
		{ immediate: true },
	);
	const user = ref<User>((store.state.user || {}) as User);
	const workspaces = ref<Workspace[]>(store.state.workspaces || []);
	const showExitConfirm = ref(false);
	const workspaceToExit = ref<Workspace | null>(null);

	async function logout() {
		try {
			if (store.getters.getPusherBeamsUserId) {
				await store.getters.getPusherBeamsClient.stop();
				store.commit('setPusherBeamsUserId', null);
			}
			await logoutAction();
			await store.dispatch('logout');
			location.reload();
		} catch (e) {
			console.error(e);
		}
	}

	const activeWorkspace = ref(workspaces.value[0]);

	const sidebarPending = ref(false),
		sidebarError = ref<string | null>(null);
	let sidebarDisposed = false,
		sidebarRequest = 0,
		sidebarBooting = 0;
	onBeforeUnmount(() => {
		sidebarDisposed = true;
		++sidebarRequest;
	});
	async function loadSidebar() {
		if (!store.getters.canUseApp) return;
		// Mounts before the router guard resolves the tab's workspace; the batch below reports getUser failures.
		let userJustLoaded = false;
		sidebarBooting++;
		try {
			if (!store.state.user?.id) {
				await getUser().catch(() => {});
				userJustLoaded = !!store.state.user?.id;
			}
			if (!store.state.workspaces?.length)
				await store.dispatch('loadWorkspaces');
		} finally {
			sidebarBooting--;
		}
		const request = ++sidebarRequest;
		const context =
			String(store.state.user?.id) +
			':' +
			String(
				store.state.user?.settings?.find(
					(s: any) => s.key === 'current_workspace',
				)?.value,
			);
		const current = () =>
			!sidebarDisposed &&
			request === sidebarRequest &&
			context ===
				String(store.state.user?.id) +
					':' +
					String(
						store.state.user?.settings?.find(
							(s: any) => s.key === 'current_workspace',
						)?.value,
					);
		sidebarPending.value = true;
		sidebarError.value = null;
		const results = await Promise.allSettled([
			(userJustLoaded
				? Promise.resolve(store.state.user as any)
				: getUser()
			).then((data) => {
				if (current()) user.value = data;
			}),
			getWorkspaces().then((data) => {
				if (current()) workspaces.value = data;
			}),
			getUserFeatureToggles().then((data) => {
				if (current()) store.commit('featureToggles/setUserToggles', data);
			}),
		]);
		if (!current()) return;
		const id = store.state.user?.settings?.find(
			(s) => s.key === 'current_workspace',
		)?.value;
		activeWorkspace.value = workspaces.value.find(
			(w) => w.id == id,
		) as Workspace;
		if (results.some((result) => result.status === 'rejected'))
			sidebarError.value = t('nav.sidebar.loadFailed');
		try {
			if (activeWorkspace.value?.id) {
				await store.dispatch(
					'featureToggles/fetchWorkspaceModules',
					activeWorkspace.value.id,
				);
			}
		} catch {
			if (current()) sidebarError.value = t('nav.sidebar.featuresFailed');
		} finally {
			if (current()) sidebarPending.value = false;
		}
	}
	onBeforeMount(loadSidebar);
	watch(
		() =>
			store.state.user?.settings?.find(
				(s: any) => s.key === 'current_workspace',
			)?.value,
		() => {
			if (sidebarBooting) return;
			void loadSidebar();
		},
	);

	// A rename or colour change commits to the store; store items win, the sidebar keeps its own membership.
	watch(
		() => store.state.workspaces,
		() => {
			const byId = store.state.workspacesById as Record<number, Workspace>;
			workspaces.value = workspaces.value.map(
				(workspace) => byId[workspace.id] ?? workspace,
			);
			const id = activeWorkspace.value?.id;
			const same = workspaces.value.find((workspace) => workspace.id === id);
			if (same) activeWorkspace.value = same;
		},
	);

	// Watch for changes to the current workspace in the store
	watch(
		() => store.state.user?.settings,
		(newSettings) => {
			if (newSettings && workspaces.value.length > 0) {
				const currentWorkspaceId = newSettings.find(
					(setting: any) => setting.key === 'current_workspace',
				)?.value;

				if (currentWorkspaceId) {
					const newActiveWorkspace = workspaces.value.find(
						(workspace: Workspace) =>
							Number(workspace.id) === Number(currentWorkspaceId),
					);

					if (
						newActiveWorkspace &&
						activeWorkspace.value?.id !== newActiveWorkspace.id
					) {
						console.log(
							`Updating active workspace in sidebar from ${activeWorkspace.value?.name} to ${newActiveWorkspace.name}`,
						);
						activeWorkspace.value = newActiveWorkspace;
					}
				}
			}
		},
		{ deep: true },
	);

	// Also watch for app rerenders
	watch(
		() => store.state.appRerenderKey,
		async () => {
			if (store.getters.canUseApp && workspaces.value.length > 0) {
				// Re-check the current workspace from settings
				const currentWorkspaceId = store.state.user?.settings?.find(
					(setting: any) => setting.key === 'current_workspace',
				)?.value;

				if (currentWorkspaceId) {
					const newActiveWorkspace = workspaces.value.find(
						(workspace: Workspace) =>
							Number(workspace.id) === Number(currentWorkspaceId),
					);

					if (newActiveWorkspace) {
						activeWorkspace.value = newActiveWorkspace;
					}
				}
			}
		},
	);

	const CATEGORY_PAGES = [
		'WorkspaceCategory',
		'WorkspaceCategoryChildren',
		'WorkspaceCategoryCreateInCategory',
	];

	const setActiveWorkspace = async (workspace: Workspace) => {
		// Update UI immediately
		activeWorkspace.value = workspace;

		try {
			let nextWorkspaceRoute: {
				path: string;
				query: typeof route.query;
				hash: string;
			} | null = null;
			const currentPath = window.location.pathname;
			const pathParts = currentPath.split('/');

			if (CATEGORY_PAGES.includes(String(route.name))) {
				nextWorkspaceRoute = {
					path: `/${workspace.code}/categories`,
					query: {},
					hash: '',
				};
			} else if (pathParts.length > 1 && route.params.workspace_code) {
				pathParts[1] = workspace.code;
				const newPath = pathParts.join('/');

				if (newPath !== route.fullPath) {
					nextWorkspaceRoute = {
						path: newPath,
						query: route.query,
						hash: route.hash,
					};
				}
			}

			if (workspace.is_local) {
				// local/install.ts activates it and keeps the server default untouched.
				const settingsWithUpdatedWorkspace = user.value?.settings.map(
					(setting) => ({
						id: setting.id,
						value:
							setting.key === 'current_workspace'
								? workspace.id
								: setting.value,
					}),
				);
				const updatedUser = await updateUserSettingsV2(
					settingsWithUpdatedWorkspace,
				);
				store.commit('setUser', updatedUser);
			} else {
				await syncActiveLocalWorkspace(workspace.id);
			}

			store.commit('updateUserWorkspaceSetting', {
				workspaceId: workspace.id,
			});

			// Load feature toggles for new workspace
			await store.dispatch('featureToggles/loadWorkspaceToggles', workspace.id);

			if (nextWorkspaceRoute) {
				await router.replace(nextWorkspaceRoute);
			}

			// Trigger UI updates
			store.commit('rerenderApp');
		} catch (error) {
			console.error('Failed to update workspace:', error);
			// If there's an error, fall back to page reload
			document.location.reload();
		}
	};

	const handleExitWorkspace = (workspace: Workspace, event: Event) => {
		event.stopPropagation();
		workspaceToExit.value = workspace;
		showExitConfirm.value = true;
	};

	const confirmExitWorkspace = async () => {
		if (!workspaceToExit.value) return;

		try {
			await exitWorkspace(workspaceToExit.value.id);
			await getUserSettings();
			window.location.reload();
		} catch (error) {
			console.error('Failed to exit workspace:', error);
			alert(serverMessageFrom(error, t('nav.sidebar.exitFailed')));
		} finally {
			showExitConfirm.value = false;
			workspaceToExit.value = null;
		}
	};

	const cancelExitWorkspace = () => {
		showExitConfirm.value = false;
		workspaceToExit.value = null;
	};

	const showLocalDialog = ref(false);
	const localName = ref('');
	const localError = ref('');
	const creatingLocal = ref(false);

	const openLocalWorkspaceDialog = () => {
		localName.value = '';
		localError.value = '';
		showLocalDialog.value = true;
	};

	const createLocalWorkspaceFromDialog = async () => {
		const name = localName.value.trim();
		if (!name || creatingLocal.value) return;
		creatingLocal.value = true;
		localError.value = '';
		try {
			const { createLocalWorkspace } = await import('@/local/runtime');
			const created = await createLocalWorkspace(name);
			requestCache.invalidate('workspaces');
			workspaces.value = await getWorkspaces(false);
			await store.dispatch('loadWorkspaces');
			showLocalDialog.value = false;
			const workspace = workspaces.value.find((w) => w.id === created.id);
			if (workspace) await setActiveWorkspace(workspace);
		} catch (error) {
			localError.value = String(error);
		} finally {
			creatingLocal.value = false;
		}
	};

	const runLocalAction = async (action: 'reveal' | 'export') => {
		try {
			const runtime = await import('@/local/runtime');
			if (action === 'reveal') await runtime.revealLocalWorkspace();
			else await runtime.exportLocalWorkspace(store.state.user?.name ?? '');
		} catch (error) {
			console.error(`Local workspace ${action} failed`, error);
		}
	};

	const moveToCloud = async () => {
		const { activeLocalWorkspace } = await import('@/local/runtime');
		const workspace = activeLocalWorkspace();
		if (workspace) openMoveDialog(workspace.code);
	};

	const canLeaveWorkspace = (workspace: Workspace) =>
		canLeaveWorkspaceFor(workspace, store.state.user?.id);
</script>

<template>
	<SidebarProvider>
		<SidebarMobileCloser>
			<Sidebar
				collapsible="icon"
				v-if="store.getters.canUseApp"
				data-tour="nav.sidebar"
			>
				<SidebarHeader>
					<AsyncContent
						:pending="sidebarPending"
						:loaded="true"
						:error="sidebarError"
						:retry="loadSidebar"
					/>
					<SidebarMenu>
						<SidebarMenuItem>
							<DropdownMenu>
								<DropdownMenuTrigger as-child>
									<SidebarMenuButton
										size="lg"
										data-tour="nav.workspace-switcher"
										class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
									>
										<div
											class="flex min-w-0 flex-1 items-center gap-2"
											@click="openWorkspaceHome"
										>
											<div
												class="flex aspect-square size-8 shrink-0 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground"
												:style="workspaceIconStyle(activeWorkspace?.color)"
											>
												<PackageOpen class="size-4" />
											</div>

											<div class="grid flex-1 text-left text-sm leading-tight">
												<span class="flex min-w-0 items-center gap-1.5">
													<span class="truncate font-semibold">
														{{ activeWorkspace?.name }}
													</span>
													<span
														v-if="activeWorkspace?.is_local"
														class="shrink-0 rounded bg-amber-500/15 px-1.5 py-0.5 text-2xs font-semibold text-amber-600 uppercase dark:text-amber-400"
														data-testid="local-workspace-badge"
														data-tour="local.badge"
														>{{ $t('nav.sidebar.local') }}</span
													>
													<span
														v-if="isDemo"
														class="shrink-0 rounded bg-violet-500/15 px-1.5 py-0.5 text-2xs font-semibold text-violet-600 uppercase dark:text-violet-400"
														data-testid="demo-badge"
														data-tour="ghost.badge"
														>{{ $t('nav.sidebar.demo') }}</span
													>
												</span>
												<span class="truncate text-xs">{{
													activeWorkspace?.is_local
														? $t('nav.sidebar.storedOnComputer')
														: $t('nav.sidebar.currentWorkspace')
												}}</span>
											</div>
										</div>
										<span
											class="ml-auto flex size-7 shrink-0 items-center justify-center rounded-md hover:bg-sidebar-border"
											:title="$t('nav.sidebar.switchWorkspace')"
										>
											<ChevronsUpDown class="size-4" />
										</span>
									</SidebarMenuButton>
								</DropdownMenuTrigger>

								<DropdownMenuContent
									class="max-h-[30rem] w-(--radix-dropdown-menu-trigger-width) min-w-56 overflow-y-auto rounded-lg"
									align="start"
									side="bottom"
									:side-offset="4"
								>
									<DropdownMenuLabel class="text-xs text-muted-foreground">
										{{ $t('nav.sidebar.workspaces') }}
									</DropdownMenuLabel>

									<DropdownMenuItem
										v-for="workspace in workspaces"
										:key="workspace.name"
										class="cursor-pointer gap-2 p-2"
										@click="setActiveWorkspace(workspace)"
									>
										<div
											class="flex size-6 items-center justify-center rounded-sm border"
											:style="workspaceIconStyle(workspace.color)"
										>
											<component
												:is="
													workspace.id === activeWorkspace.id
														? PackageOpen
														: Package
												"
												class="size-4 shrink-0"
											/>
										</div>

										<span class="flex-1">{{ workspace.name }}</span>
										<span
											v-if="workspace.is_local"
											class="rounded bg-muted px-1.5 py-0.5 text-2xs font-semibold text-muted-foreground dark:bg-muted"
											:title="workspace.path"
											>{{ $t('nav.sidebar.local') }}</span
										>

										<button
											v-if="canLeaveWorkspace(workspace)"
											@click.stop="handleExitWorkspace(workspace, $event)"
											class="ml-auto flex size-6 items-center justify-center rounded transition-colors hover:bg-destructive/10 hover:text-destructive"
											:title="$t('nav.sidebar.leaveWorkspace')"
										>
											<LogOut class="size-4" />
										</button>
									</DropdownMenuItem>

									<DropdownMenuSeparator />

									<DropdownMenuItem
										class="cursor-pointer gap-2 p-2"
										@click="$router.push('/settings/workspaces?create')"
									>
										<div
											class="flex size-6 items-center justify-center rounded-md border bg-background"
										>
											<Plus class="size-4" />
										</div>
										<div class="font-medium text-muted-foreground">
											{{ $t('nav.sidebar.addWorkspace') }}
										</div>
									</DropdownMenuItem>

									<template v-if="isDesktop && activeWorkspace?.is_local">
										<DropdownMenuItem
											class="cursor-pointer gap-2 p-2"
											@click="runLocalAction('reveal')"
										>
											<div
												class="flex size-6 items-center justify-center rounded-md border bg-background"
											>
												<FolderOpen class="size-4" />
											</div>
											<div class="font-medium text-muted-foreground">
												{{ revealLabel }}
											</div>
										</DropdownMenuItem>
										<DropdownMenuItem
											class="cursor-pointer gap-2 p-2"
											@click="runLocalAction('export')"
										>
											<div
												class="flex size-6 items-center justify-center rounded-md border bg-background"
											>
												<FileDown class="size-4" />
											</div>
											<div class="font-medium text-muted-foreground">
												{{ $t('nav.sidebar.exportMarkdown') }}
											</div>
										</DropdownMenuItem>
										<DropdownMenuItem
											v-if="!isLocalOnly && !isDemo"
											class="cursor-pointer gap-2 p-2"
											data-testid="move-to-cloud-menu-item"
											@click="moveToCloud"
										>
											<div
												class="flex size-6 items-center justify-center rounded-md border bg-background"
											>
												<CloudUpload class="size-4" />
											</div>
											<div class="font-medium text-muted-foreground">
												{{ $t('nav.sidebar.moveToCloud') }}
											</div>
										</DropdownMenuItem>
									</template>

									<DropdownMenuItem
										v-if="isDesktop"
										class="cursor-pointer gap-2 p-2"
										@click="openLocalWorkspaceDialog"
									>
										<div
											class="flex size-6 items-center justify-center rounded-md border bg-background"
										>
											<HardDrive class="size-4" />
										</div>
										<div class="font-medium text-muted-foreground">
											{{ $t('nav.sidebar.newLocalWorkspace') }}
										</div>
									</DropdownMenuItem>

									<DropdownMenuItem
										class="cursor-pointer gap-2 p-2"
										@click="$router.push('/settings/workspaces?invite')"
									>
										<div
											class="flex size-6 items-center justify-center rounded-md border bg-background"
										>
											<UserPlus class="size-4" />
										</div>
										<div class="font-medium text-muted-foreground">
											{{ $t('nav.sidebar.inviteMembers') }}
										</div>
									</DropdownMenuItem>
								</DropdownMenuContent>
							</DropdownMenu>
						</SidebarMenuItem>
					</SidebarMenu>
					<SidebarMenu v-if="!isMobile" data-testid="sidebar-quick-actions">
						<SidebarMenuItem>
							<GlobalSearch variant="sidebar" />
						</SidebarMenuItem>
					</SidebarMenu>
				</SidebarHeader>

				<SidebarContent>
					<SidebarGroup>
						<SidebarGroupLabel>{{ $t('nav.sidebar.main') }}</SidebarGroupLabel>

						<SidebarMenu>
							<SidebarMenuItem v-if="isFeatureEnabled('dashboard')">
								<SidebarMenuButton as-child>
									<router-link
										:class="NAV_ACTIVE"
										:to="
											activeWorkspace?.code
												? `/${activeWorkspace.code}/dashboard`
												: '/app'
										"
									>
										<LayoutDashboard />
										<span>{{ $t('nav.sidebar.dashboard') }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>

							<SidebarMenuItem>
								<SidebarMenuButton as-child>
									<router-link
										:class="NAV_ACTIVE"
										:to="
											activeWorkspace?.code
												? `/${activeWorkspace.code}/list`
												: '/list'
										"
									>
										<ClipboardListIcon />
										<span>{{ $t('nav.sidebar.list') }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>

							<SidebarMenuItem v-if="showSurface('board.nav')">
								<SidebarMenuButton as-child>
									<router-link
										:class="NAV_ACTIVE"
										:to="
											activeWorkspace?.code
												? `/${activeWorkspace.code}/board`
												: '/board'
										"
									>
										<SquareKanban />
										<span>{{ $t('nav.sidebar.board') }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>

							<SidebarMenuItem v-if="isFeatureEnabled('daily_routines')">
								<SidebarMenuButton
									as-child
									:tooltip="
										activeWorkspace?.is_local
											? $t('nav.sidebar.localRoutinesTooltip')
											: undefined
									"
								>
									<router-link
										to="/routines"
										:class="NAV_ACTIVE"
										:title="
											activeWorkspace?.is_local
												? $t('nav.sidebar.localRoutinesTooltip')
												: undefined
										"
									>
										<Inbox />
										<span class="flex min-w-0 items-center gap-1.5">
											<span class="truncate">{{
												activeWorkspace?.is_local
													? $t('nav.sidebar.localRoutines')
													: $t('nav.sidebar.dailyRoutines')
											}}</span>
											<span
												v-if="activeWorkspace?.is_local"
												class="shrink-0 rounded bg-amber-500/15 px-1.5 py-0.5 text-2xs font-semibold text-amber-600 uppercase dark:text-amber-400"
												data-testid="local-routines-menu-badge"
												>{{ $t('nav.sidebar.local') }}</span
											>
										</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>

							<SidebarMenuItem>
								<SidebarMenuButton as-child>
									<router-link
										data-tour="nav.archive"
										:class="NAV_ACTIVE"
										:to="
											activeWorkspace?.code
												? `/${activeWorkspace.code}/archive`
												: '/archive'
										"
									>
										<ArchiveIcon />
										<span>{{ $t('nav.sidebar.archive') }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
						</SidebarMenu>
					</SidebarGroup>

					<SidebarCategoriesSection
						v-if="isFeatureEnabled('categories') && activeWorkspace?.code"
						:workspace-code="activeWorkspace.code"
					/>

					<SidebarPagesSection
						v-if="pagesEnabled && activeWorkspace?.code"
						:workspace-id="Number(activeWorkspace.id)"
						:workspace-code="activeWorkspace.code"
					/>

					<SidebarGroup>
						<SidebarGroupLabel>{{ $t('nav.sidebar.tools') }}</SidebarGroupLabel>

						<SidebarMenu>
							<SidebarMenuItem>
								<SidebarMenuButton as-child>
									<router-link to="/stats" :class="NAV_ACTIVE">
										<BarChart3 />
										<span>{{ $t('nav.sidebar.statistics') }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
							<SidebarMenuItem v-if="isDesktop">
								<SidebarMenuButton as-child>
									<router-link to="/ai-usage" :class="NAV_ACTIVE">
										<Gauge />
										<span>{{ $t('nav.sidebar.aiUsage') }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
							<SidebarMenuItem
								v-if="activeWorkspace?.code && showSurface('files.nav')"
							>
								<SidebarMenuButton as-child>
									<router-link
										:to="`/${activeWorkspace.code}/files`"
										:class="NAV_ACTIVE"
									>
										<PaperclipIcon />
										<span>{{ $t('nav.sidebar.files') }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
							<SidebarMenuItem
								v-if="activeWorkspace?.code && pagesEnabled && !isLocalOnly"
							>
								<SidebarMenuButton as-child>
									<router-link
										:to="`/${activeWorkspace.code}/memory`"
										:class="NAV_ACTIVE"
									>
										<Brain />
										<span>{{ $t('nav.sidebar.memory') }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
							<SidebarMenuItem
								v-if="activeWorkspace?.code && isFeatureEnabled('graph')"
							>
								<SidebarMenuButton as-child>
									<router-link
										:to="`/${activeWorkspace.code}/map`"
										:class="NAV_ACTIVE"
									>
										<Waypoints />
										<span>{{ $t('nav.sidebar.map') }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
							<template v-if="activeWorkspace?.code">
								<SidebarMenuItem v-for="page in pluginPages" :key="page.key">
									<SidebarMenuButton
										as-child
										:class="
											page.badge && 'pr-11 group-data-[collapsible=icon]:!pr-2'
										"
									>
										<router-link
											:to="`/${activeWorkspace.code}/plugins/${page.key}`"
											:class="NAV_ACTIVE"
											:aria-label="viewBadgeAriaLabel(page.title, page.badge)"
										>
											<span class="relative flex shrink-0">
												<Plug />
												<span
													v-if="page.badge"
													aria-hidden="true"
													data-testid="plugin-view-badge-dot"
													:class="[
														'absolute -top-0.5 -right-0.5 hidden size-2 rounded-full ring-2 ring-sidebar group-data-[collapsible=icon]:block',
														BADGE_DOT_CLASS[page.badge.tone],
													]"
												/>
											</span>
											<span>{{ page.title }}</span>
										</router-link>
									</SidebarMenuButton>
									<SidebarMenuBadge
										v-if="page.badge"
										aria-hidden="true"
										data-testid="plugin-view-badge"
										:class="[
											'rounded-full px-1.5 text-2xs font-semibold',
											BADGE_TONE_CLASS[page.badge.tone],
										]"
									>
										{{ viewBadgeLabel(page.badge) }}
									</SidebarMenuBadge>
								</SidebarMenuItem>
							</template>
							<SidebarMenuItem v-if="activeWorkspace?.code && !hubHidden">
								<SidebarMenuButton as-child>
									<router-link
										data-tour="nav.getting-started"
										:to="`/${activeWorkspace.code}/getting-started`"
										:class="NAV_ACTIVE"
									>
										<LifeBuoy />
										<span>{{ $t('nav.sidebar.gettingStarted') }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
						</SidebarMenu>
					</SidebarGroup>
				</SidebarContent>

				<SidebarFooter>
					<SidebarMenu>
						<SidebarMenuItem
							v-if="isLocalOnly"
							class="group-data-[collapsible=icon]:hidden"
						>
							<Button
								class="w-full"
								size="sm"
								data-testid="sidebar-create-account"
								data-tour="nav.create-account"
								@click="$router.push({ name: 'Register' })"
							>
								{{ $t('nav.sidebar.createAccount') }}
							</Button>
						</SidebarMenuItem>
						<SidebarMenuItem>
							<DropdownMenu>
								<DropdownMenuTrigger as-child>
									<SidebarMenuButton
										size="lg"
										data-tour="nav.user-menu"
										class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
									>
										<UserAvatar
											:user-id="user?.id ?? 0"
											:name="user?.name ?? ''"
											:has-avatar="user?.has_avatar ?? false"
											:size="32"
										/>
										<div class="grid flex-1 text-left text-sm leading-tight">
											<span class="flex items-center gap-1.5">
												<span class="truncate font-semibold">{{
													user.name
												}}</span>
												<span
													v-if="isLocalOnly"
													data-testid="local-badge"
													class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-2xs font-medium text-muted-foreground"
													>{{ $t('nav.sidebar.local') }}</span
												>
											</span>
											<span v-if="!isLocalOnly" class="truncate text-xs">{{
												user.email
											}}</span>
										</div>
										<ChevronsUpDown class="ml-auto size-4" />
									</SidebarMenuButton>
								</DropdownMenuTrigger>

								<DropdownMenuContent
									class="w-(--radix-dropdown-menu-trigger-width) min-w-56 rounded-lg"
									side="bottom"
									align="end"
									:side-offset="4"
								>
									<DropdownMenuLabel class="p-0 font-normal">
										<div
											class="flex items-center gap-2 px-1 py-1.5 text-left text-sm"
										>
											<UserAvatar
												:user-id="user?.id ?? 0"
												:name="user?.name ?? ''"
												:has-avatar="user?.has_avatar ?? false"
												:size="32"
											/>
											<div class="grid flex-1 text-left text-sm leading-tight">
												<span class="truncate font-semibold">
													{{ user.name }}
												</span>
												<span v-if="!isLocalOnly" class="truncate text-xs">{{
													user.email
												}}</span>
											</div>
										</div>
									</DropdownMenuLabel>

									<DropdownMenuSeparator />

									<DropdownMenuGroup>
										<DropdownMenuItem
											@click="$router.push('/settings?tab=profile')"
											class="cursor-pointer"
										>
											<BadgeCheck /> {{ $t('nav.sidebar.profile') }}
										</DropdownMenuItem>
										<DropdownMenuItem
											@click="$router.push('/settings/sections')"
											class="cursor-pointer"
										>
											<Settings2 /> {{ $t('nav.sidebar.settings') }}
										</DropdownMenuItem>
										<DropdownMenuItem
											v-if="activeWorkspace?.code && hubHidden"
											data-tour="nav.getting-started"
											@click="
												$router.push(`/${activeWorkspace.code}/getting-started`)
											"
											class="cursor-pointer"
										>
											<LifeBuoy /> {{ $t('nav.sidebar.gettingStarted') }}
										</DropdownMenuItem>
										<DropdownMenuItem>
											<DarkMode />
										</DropdownMenuItem>
									</DropdownMenuGroup>

									<template v-if="!isLocalOnly">
										<DropdownMenuSeparator />

										<DropdownMenuItem class="cursor-pointer" @click="logout">
											<LogOut /> {{ $t('nav.sidebar.logout') }}
										</DropdownMenuItem>
									</template>
								</DropdownMenuContent>
							</DropdownMenu>
						</SidebarMenuItem>
					</SidebarMenu>
				</SidebarFooter>
				<SidebarRail />
			</Sidebar>

			<SidebarInset
				class="app-canvas pt-[var(--titlebar-h,0px)] pb-[var(--statusbar-h,0px)]"
			>
				<DesktopAppBar />
				<EmailVerifyBanner />
				<LocalOnlyBanner v-if="isDesktop" />
				<LocalMoveBanner v-if="isDesktop && !isLocalOnly" />
				<header
					v-if="store.getters.canUseApp"
					data-tauri-drag-region
					:class="
						isDesktop
							? 'fixed inset-x-0 top-0 z-30 flex h-[var(--titlebar-h)] items-center gap-2 bg-sidebar'
							: 'flex h-16 shrink-0 items-center gap-2 transition-[width,height] ease-linear group-has-[[data-collapsible=icon]]/sidebar-wrapper:h-12'
					"
				>
					<div
						data-tauri-drag-region
						class="flex flex-1 items-center gap-2 px-4"
						:class="{ 'md:px-6': !isDesktop }"
					>
						<div
							v-if="showWindowControls"
							data-tauri-drag-region
							class="-ml-1 flex w-[calc(var(--sidebar-width)-1rem)] shrink-0 items-center group-has-[[data-collapsible=icon]]/sidebar-wrapper:mr-1 group-has-[[data-collapsible=icon]]/sidebar-wrapper:w-auto"
						>
							<WindowControls />
						</div>
						<SidebarTrigger class="-ml-1" data-tour="nav.menu-button" />
						<div v-if="isDesktop" class="flex items-center">
							<button
								type="button"
								class="flex h-8 w-8 items-center justify-center rounded-md text-ink-subtle transition hover:bg-surface-hover hover:text-ink disabled:pointer-events-none disabled:opacity-40"
								:aria-label="$t('nav.sidebar.back')"
								:title="$t('nav.sidebar.back')"
								:disabled="!canBack"
								@click="navHistory.back()"
							>
								<ArrowLeft class="h-4 w-4" />
							</button>
							<button
								type="button"
								class="flex h-8 w-8 items-center justify-center rounded-md text-ink-subtle transition hover:bg-surface-hover hover:text-ink disabled:pointer-events-none disabled:opacity-40"
								:aria-label="$t('nav.sidebar.forward')"
								:title="$t('nav.sidebar.forward')"
								:disabled="!canForward"
								@click="navHistory.forward()"
							>
								<ArrowRight class="h-4 w-4" />
							</button>
						</div>
						<AddTaskModalTrigger
							class="-ml-1"
							:aria-label="$t('nav.sidebar.newTask')"
							:title="$t('nav.sidebar.newTask')"
							data-tour="nav.create-task"
						/>
						<GlobalSearch v-if="isMobile" />

						<Separator
							data-tauri-drag-region
							orientation="vertical"
							class="mr-2 h-4"
						/>

						<span
							v-if="isDesktop"
							data-tauri-drag-region
							class="truncate text-sm font-semibold"
						>
							{{ metaTitle }}
						</span>
						<Breadcrumb v-else>
							<BreadcrumbList>
								<BreadcrumbItem class="hidden md:block">
									<BreadcrumbLink>
										<router-link
											:to="
												activeWorkspace?.code
													? `/${activeWorkspace.code}/list`
													: '/list'
											"
										>
											TMGR.DEV
										</router-link>
									</BreadcrumbLink>
								</BreadcrumbItem>

								<BreadcrumbSeparator class="hidden md:block" />

								<BreadcrumbItem class="hidden md:block">
									{{ metaTitle }}
								</BreadcrumbItem>
							</BreadcrumbList>
						</Breadcrumb>

						<div
							id="page-header-actions"
							data-tauri-drag-region
							class="ml-auto flex min-w-0 flex-1 items-center justify-end gap-2"
						></div>
						<div data-tauri-drag-region class="flex items-center gap-2">
							<button
								v-if="showAskAi"
								class="flex h-8 w-8 items-center justify-center rounded-pill text-ink-subtle transition hover:bg-surface-hover hover:text-ink"
								:class="{
									'bg-surface-hover text-ink': store.state.aiPanelOpen,
								}"
								:aria-label="$t('nav.sidebar.askAi')"
								:title="$t('nav.sidebar.askAi')"
								data-tour="nav.ask-ai"
								@click="store.commit('toggleAiPanel')"
							>
								<Sparkles class="h-4 w-4" />
							</button>
							<ActiveCursorAgents
								v-if="!isDesktop && isFeatureEnabled('cursor')"
							/>
							<HelpMenu />
							<NotificationBell v-if="!isLocalOnly" />
						</div>
					</div>
				</header>

				<div
					class="flex min-h-max flex-1 flex-col gap-4 max-sm:overflow-x-clip"
				>
					<slot />
				</div>
			</SidebarInset>
			<LocalAgentPanel
				v-if="isDesktop && isLocalWorkspace && isFeatureEnabled('ai.assistant')"
			/>
			<AiAssistantPanel
				v-else-if="
					store.getters.isLoggedIn &&
					!isLocalOnly &&
					!isLocalWorkspace &&
					isFeatureEnabled('ai.assistant')
				"
			/>
			<ModulesPicker v-if="store.getters.isLoggedIn && !isLocalOnly" />
		</SidebarMobileCloser>
	</SidebarProvider>

	<!-- Exit Workspace Confirm Dialog -->
	<Dialog v-if="isDesktop" v-model:open="showLocalDialog">
		<DialogContent class="max-w-md">
			<DialogHeader>
				<DialogTitle>{{ $t('nav.sidebar.newLocalWorkspace') }}</DialogTitle>
				<DialogDescription>
					{{ $t('nav.sidebar.localDialogDescription') }}
				</DialogDescription>
			</DialogHeader>
			<form
				class="flex flex-col gap-3"
				@submit.prevent="createLocalWorkspaceFromDialog"
			>
				<input
					v-model="localName"
					data-selectable
					autofocus
					class="h-9 rounded-md border border-input bg-background px-3 text-sm outline-hidden dark:border-input dark:bg-background"
					:placeholder="$t('nav.sidebar.localNamePlaceholder')"
				/>
				<p v-if="localError" class="text-xs text-destructive">
					{{ localError }}
				</p>
				<DialogFooter>
					<button
						type="submit"
						class="h-9 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
						:disabled="!localName.trim() || creatingLocal"
					>
						{{
							creatingLocal
								? $t('nav.sidebar.creating')
								: $t('common.actions.create')
						}}
					</button>
				</DialogFooter>
			</form>
		</DialogContent>
	</Dialog>

	<MoveToCloudHost v-if="isDesktop && !isLocalOnly" />
	<LocalUpgradeHost v-if="isDesktop && !isLocalOnly" />

	<Confirm
		v-if="showExitConfirm"
		:title="$t('nav.sidebar.leaveWorkspace')"
		:body="$t('nav.sidebar.leaveConfirm', { name: workspaceToExit?.name })"
		@on-ok="confirmExitWorkspace"
		@on-cancel="cancelExitWorkspace"
	/>
</template>
