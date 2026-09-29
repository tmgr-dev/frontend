<script setup lang="ts">
	import {
		getUserFeatureToggles,
		getWorkspaceFeatureToggles,
	} from '@/actions/tmgr/featureToggles';
	import AsyncContent from '@/components/async/AsyncContent.vue';

	import WindowControls from '@/components/desktop/WindowControls.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import { isDesktopApp } from '@/utils/desktop';

	import { logout as logoutAction } from '@/actions/tmgr/auth.ts';
	import { Category, getTopCategories } from '@/actions/tmgr/categories.ts';
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
	import ActiveCursorAgents from '@/components/cursor/ActiveCursorAgents.vue';
	import Confirm from '@/components/general/Confirm.vue';
	import DarkMode from '@/components/general/DarkMode.vue';
	import NotificationBell from '@/components/notifications/NotificationBell.vue';
	import {
		Breadcrumb,
		BreadcrumbItem,
		BreadcrumbLink,
		BreadcrumbList,
		BreadcrumbSeparator,
	} from '@/components/ui/breadcrumb';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuGroup,
		DropdownMenuItem,
		DropdownMenuLabel,
		DropdownMenuSeparator,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import {
		Dialog,
		DialogContent,
		DialogDescription,
		DialogFooter,
		DialogHeader,
		DialogTitle,
	} from '@/components/ui/dialog';
	import { Separator } from '@/components/ui/separator';
	import { syncActiveLocalWorkspace } from '@/utils/localWorkspaceSync';
	import { requestCache } from '@/utils/requestCache';
	import {
		Sidebar,
		SidebarContent,
		SidebarFooter,
		SidebarGroup,
		SidebarGroupLabel,
		SidebarHeader,
		SidebarInset,
		SidebarMenu,
		SidebarMenuButton,
		SidebarMenuItem,
		SidebarProvider,
		SidebarRail,
		SidebarTrigger,
	} from '@/components/ui/sidebar';
	import AddTaskModalTrigger from '@/components/ui/sidebar/AddTaskModalTrigger.vue';
	import SidebarMobileCloser from '@/components/ui/sidebar/SidebarMobileCloser.vue';
	import { setDocumentTitle } from '@/composable/useDocumentTitle';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';
	import store from '@/store';
	import { generateCategoryUrl, generateWorkspaceUrl } from '@/utils/url';
	import { pluginState } from '@/pluginSystem/state';
	import {
		ArchiveIcon,
		BadgeCheck,
		BarChart3,
		Bell,
		Cable,
		ChevronsUpDown,
		ClipboardListIcon,
		FolderClosedIcon,
		Inbox,
		FileDown,
		FolderOpen,
		HardDrive,
		LayoutDashboard,
		LogOut,
		Package,
		PackageOpen,
		Keyboard,
		Palette,
		PaperclipIcon,
		Plug,
		Plus,
		Settings2,
		Sliders,
		Sparkles,
		SquareKanban,
		UserPlus,
		VenetianMask,
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

	const AiAssistantPanel = defineAsyncComponent(
		() => import('@/components/agent/AiAssistantPanel.vue'),
	);

	const route = useRoute();
	const router = useRouter();
	const { isFeatureEnabled, isUserFeatureEnabled } = useFeatureToggles();

	const isDesktop = isDesktopApp();
	const pluginPages = computed(() =>
		Object.values(pluginState.plugins)
			.filter((plugin) => plugin.status === 'running')
			.flatMap((plugin) =>
				plugin.manifest.contributes.views.map((view) => ({
					key: `${plugin.manifest.id}/${view.id}`,
					title: view.title,
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
		router.push('/');
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
				store.commit('setMetaTitle', newTitle);
				setDocumentTitle(newTitle as string);
			}
		},
		{ immediate: true },
	);
	const user = ref<User>((store.state.user || {}) as User);
	const categories = ref<Category[]>([]);
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
		categoriesLoaded = ref(false),
		sidebarError = ref<string | null>(null);
	let sidebarDisposed = false,
		sidebarRequest = 0;
	onBeforeUnmount(() => {
		sidebarDisposed = true;
		++sidebarRequest;
	});
	async function loadSidebar() {
		if (!store.getters.isLoggedIn) return;
		// This can run before the router guard's own resolution finishes (the sidebar mounts
		// once at app boot, not per-navigation) — without this, getTopCategories() below can
		// fire before state.workspaces is known and go out with no X-Workspace-Id header.
		// Swallow: the batch below re-calls getUser() and records a failure into sidebarError
		// the same way it always did; an unhandled rejection here would escape onBeforeMount.
		if (!store.state.user?.id) await getUser().catch(() => {});
		if (!store.state.workspaces?.length) await store.dispatch('loadWorkspaces');
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
			getTopCategories().then((rows) => {
				if (current()) {
					categories.value = rows.slice(0, 4);
					categoriesLoaded.value = true;
				}
			}),
			getUser().then((data) => {
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
		// store.state.user, not the local `user` ref: getUser() already committed it, overlaid
		// with this tab's workspace, while the ref above still holds the raw server response.
		const id = store.state.user?.settings?.find(
			(s) => s.key === 'current_workspace',
		)?.value;
		activeWorkspace.value = workspaces.value.find(
			(w) => w.id == id,
		) as Workspace;
		if (results.some((result) => result.status === 'rejected'))
			sidebarError.value = 'Some navigation data could not be loaded.';
		try {
			if (activeWorkspace.value?.id) {
				const data = await getWorkspaceFeatureToggles(activeWorkspace.value.id);
				if (current()) store.commit('featureToggles/setWorkspaceToggles', data);
			}
		} catch {
			if (current()) sidebarError.value = 'Could not load workspace features.';
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
			categories.value = [];
			categoriesLoaded.value = false;
			void loadSidebar();
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
			if (store.getters.isLoggedIn && workspaces.value.length > 0) {
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

			if (pathParts.length > 1 && route.params.workspace_code) {
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
				// Desktop-only path: local/install.ts's settingsAdapter activates the local
				// workspace and keeps the server's own current_workspace untouched.
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

			// This tab's workspace changes locally; nothing is sent to the server.
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
			alert('Failed to exit workspace. Please try again.');
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

	const canLeaveWorkspace = (workspace: Workspace) => {
		if (workspace.is_local) {
			return false;
		}
		// Can't leave default workspace
		if (workspace.type === 'default' || workspace.is_default) {
			return false;
		}
		// Can leave if not the owner
		if (workspace.user_id && workspace.user_id !== store.state.user?.id) {
			return true;
		}
		// Show for all non-default workspaces
		return true;
	};

	// Get the current workspace from store
	const currentWorkspace = computed(() => {
		const currentWorkspaceId = store.state.user?.settings?.find(
			(setting: any) => setting.key === 'current_workspace',
		)?.value;

		return store.state.workspaces.find(
			(workspace: any) => Number(workspace.id) === Number(currentWorkspaceId),
		);
	});

	const archiveUrl = computed(() =>
		generateWorkspaceUrl('archive', activeWorkspace.value),
	);
</script>

<template>
	<SidebarProvider>
		<SidebarMobileCloser>
			<Sidebar collapsible="icon" v-if="store.getters.isLoggedIn">
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
										class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
									>
										<div
											class="flex min-w-0 flex-1 items-center gap-2"
											@click="openWorkspaceHome"
										>
											<div
												class="flex aspect-square size-8 shrink-0 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground"
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
														class="shrink-0 rounded bg-amber-500/15 px-1.5 py-0.5 text-2xs font-semibold uppercase text-amber-600 dark:text-amber-400"
														data-testid="local-workspace-badge"
														>Local</span
													>
												</span>
												<span class="truncate text-xs">{{
													activeWorkspace?.is_local
														? 'stored on this computer'
														: 'current workspace'
												}}</span>
											</div>
										</div>
										<span
											class="ml-auto flex size-7 shrink-0 items-center justify-center rounded-md hover:bg-sidebar-border"
											title="Switch workspace"
										>
											<ChevronsUpDown class="size-4" />
										</span>
									</SidebarMenuButton>
								</DropdownMenuTrigger>

								<DropdownMenuContent
									class="max-h-[30rem] w-[--radix-dropdown-menu-trigger-width] min-w-56 overflow-y-auto rounded-lg"
									align="start"
									side="bottom"
									:side-offset="4"
								>
									<DropdownMenuLabel class="text-xs text-muted-foreground">
										Workspaces
									</DropdownMenuLabel>

									<DropdownMenuItem
										v-for="workspace in workspaces"
										:key="workspace.name"
										class="cursor-pointer gap-2 p-2"
										@click="setActiveWorkspace(workspace)"
									>
										<div
											class="flex size-6 items-center justify-center rounded-sm border"
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
											>Local</span
										>

										<button
											v-if="canLeaveWorkspace(workspace)"
											@click.stop="handleExitWorkspace(workspace, $event)"
											class="ml-auto flex size-6 items-center justify-center rounded transition-colors hover:bg-destructive/10 hover:text-destructive"
											title="Leave workspace"
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
											Add workspace
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
												Show in Finder
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
												Export to Markdown
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
											New local workspace
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
											Invite members
										</div>
									</DropdownMenuItem>
								</DropdownMenuContent>
							</DropdownMenu>
						</SidebarMenuItem>
					</SidebarMenu>
				</SidebarHeader>

				<SidebarContent>
					<SidebarGroup>
						<SidebarGroupLabel>TMGR.DEV</SidebarGroupLabel>

						<SidebarMenu>
							<SidebarMenuItem v-if="isFeatureEnabled('dashboard')">
								<SidebarMenuButton as-child>
									<router-link
										:to="
											activeWorkspace?.code
												? `/${activeWorkspace.code}/dashboard`
												: '/'
										"
									>
										<LayoutDashboard />
										<span>Dashboard</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>

							<SidebarMenuItem>
								<SidebarMenuButton as-child>
									<router-link
										:to="
											activeWorkspace?.code
												? `/${activeWorkspace.code}/list`
												: '/list'
										"
									>
										<ClipboardListIcon />
										<span>List</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>

							<SidebarMenuItem v-if="isFeatureEnabled('board')">
								<SidebarMenuButton as-child>
									<router-link
										:to="
											activeWorkspace?.code
												? `/${activeWorkspace.code}/board`
												: '/board'
										"
									>
										<SquareKanban />
										<span>Board</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>

							<SidebarMenuItem v-if="isFeatureEnabled('daily_routines')">
								<SidebarMenuButton
									as-child
									:tooltip="
										activeWorkspace?.is_local
											? 'Stored only on this device — not synced'
											: undefined
									"
								>
									<router-link
										to="/routines"
										:title="
											activeWorkspace?.is_local
												? 'Stored only on this device — not synced'
												: undefined
										"
									>
										<Inbox />
										<span class="flex min-w-0 items-center gap-1.5">
											<span class="truncate">{{
												activeWorkspace?.is_local
													? 'Local routines'
													: 'Daily Routines'
											}}</span>
											<span
												v-if="activeWorkspace?.is_local"
												class="shrink-0 rounded bg-amber-500/15 px-1.5 py-0.5 text-2xs font-semibold uppercase text-amber-600 dark:text-amber-400"
												data-testid="local-routines-menu-badge"
												>Local</span
											>
										</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>

							<SidebarMenuItem v-if="isFeatureEnabled('categories')">
								<SidebarMenuButton as-child>
									<router-link
										:to="
											activeWorkspace?.code
												? `/${activeWorkspace.code}/categories`
												: '/projects-categories'
										"
									>
										<FolderClosedIcon />
										<span>Categories</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
						</SidebarMenu>
					</SidebarGroup>

					<SidebarGroup
						v-if="isFeatureEnabled('categories')"
						class="group-data-[collapsible=icon]:hidden"
					>
						<SidebarGroupLabel>Recent categories</SidebarGroupLabel>
						<AsyncContent
							:pending="sidebarPending"
							:loaded="categoriesLoaded"
							:error="sidebarError"
							label="Loading categories"
						>
							<SidebarMenu>
								<SidebarMenuItem v-for="item in categories" :key="item.title">
									<SidebarMenuButton as-child>
										<router-link
											:to="
												activeWorkspace?.code
													? generateCategoryUrl(item.id, activeWorkspace)
													: `/projects-categories/${item.id}/children`
											"
										>
											<span>{{ item.title }}</span>
										</router-link>
									</SidebarMenuButton>
								</SidebarMenuItem>
							</SidebarMenu>
						</AsyncContent>
					</SidebarGroup>

					<SidebarGroup>
						<SidebarGroupLabel>More</SidebarGroupLabel>

						<SidebarMenu>
							<SidebarMenuItem v-if="activeWorkspace?.code">
								<SidebarMenuButton as-child>
									<router-link :to="`/${activeWorkspace.code}/files`">
										<PaperclipIcon />
										<span>Files</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
							<SidebarMenuItem>
								<SidebarMenuButton as-child>
									<router-link
										:to="
											activeWorkspace?.code
												? `/${activeWorkspace.code}/archive`
												: '/archive'
										"
									>
										<ArchiveIcon />
										<span>Archive</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
						</SidebarMenu>
					</SidebarGroup>

					<SidebarGroup v-if="pluginPages.length && activeWorkspace?.code">
						<SidebarGroupLabel>Plugins</SidebarGroupLabel>
						<SidebarMenu>
							<SidebarMenuItem v-for="page in pluginPages" :key="page.key">
								<SidebarMenuButton as-child>
									<router-link
										:to="`/${activeWorkspace.code}/plugins/${page.key}`"
									>
										<Plug />
										<span>{{ page.title }}</span>
									</router-link>
								</SidebarMenuButton>
							</SidebarMenuItem>
						</SidebarMenu>
					</SidebarGroup>
				</SidebarContent>

				<SidebarFooter>
					<SidebarMenu>
						<SidebarMenuItem>
							<DropdownMenu>
								<DropdownMenuTrigger as-child>
									<SidebarMenuButton
										size="lg"
										class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
									>
										<UserAvatar
											:user-id="user?.id ?? 0"
											:name="user?.name ?? ''"
											:has-avatar="user?.has_avatar ?? false"
											:size="32"
										/>
										<div class="grid flex-1 text-left text-sm leading-tight">
											<span class="truncate font-semibold">{{
												user.name
											}}</span>
											<span class="truncate text-xs">{{ user.email }}</span>
										</div>
										<ChevronsUpDown class="ml-auto size-4" />
									</SidebarMenuButton>
								</DropdownMenuTrigger>

								<DropdownMenuContent
									class="w-[--radix-dropdown-menu-trigger-width] min-w-56 rounded-lg"
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
												<span class="truncate text-xs">{{ user.email }}</span>
											</div>
										</div>
									</DropdownMenuLabel>

									<DropdownMenuSeparator />

									<DropdownMenuGroup>
										<DropdownMenuItem
											@click="$router.push('/settings?tab=profile')"
											class="cursor-pointer"
										>
											<BadgeCheck />
											Profile
										</DropdownMenuItem>
										<DropdownMenuItem
											@click="$router.push('/settings?tab=device')"
											class="cursor-pointer"
										>
											<Cable />
											Smart Devices
										</DropdownMenuItem>
										<DropdownMenuItem
											@click="$router.push('/settings?tab=notification')"
											class="cursor-pointer"
										>
											<Bell />
											Notifications
										</DropdownMenuItem>
										<DropdownMenuItem
											@click="$router.push('/settings/workspaces')"
											class="cursor-pointer"
										>
											<Settings2 />
											Workspace Settings
										</DropdownMenuItem>
										<DropdownMenuItem
											@click="$router.push('/settings/features')"
											class="cursor-pointer"
										>
											<Sliders />
											Feature Settings
										</DropdownMenuItem>
										<DropdownMenuItem
											@click="$router.push('/settings/personas')"
											class="cursor-pointer"
										>
											<VenetianMask />
											Personas
										</DropdownMenuItem>
										<DropdownMenuItem
											v-if="isDesktop"
											@click="$router.push('/settings/plugins')"
											class="cursor-pointer"
										>
											<Plug />
											Plugins
										</DropdownMenuItem>
										<DropdownMenuItem
											@click="$router.push('/stats')"
											class="cursor-pointer"
										>
											<BarChart3 />
											Statistics
										</DropdownMenuItem>
										<DropdownMenuItem
											@click="$router.push('/settings?tab=theme')"
											class="cursor-pointer"
										>
											<Palette />
											Theme
										</DropdownMenuItem>
										<DropdownMenuItem
											v-if="isDesktop"
											@click="$router.push('/settings?tab=desktop')"
											class="cursor-pointer"
										>
											<Keyboard />
											Shortcuts
										</DropdownMenuItem>
										<DropdownMenuItem>
											<DarkMode />
										</DropdownMenuItem>
									</DropdownMenuGroup>

									<DropdownMenuSeparator />

									<DropdownMenuItem class="cursor-pointer" @click="logout">
										<LogOut />
										Log out
									</DropdownMenuItem>
								</DropdownMenuContent>
							</DropdownMenu>
						</SidebarMenuItem>
					</SidebarMenu>
				</SidebarFooter>
				<SidebarRail />
			</Sidebar>

			<SidebarInset
				class="app-canvas pb-[var(--statusbar-h,0px)] pt-[var(--titlebar-h,0px)]"
			>
				<header
					v-if="store.getters.isLoggedIn"
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
							v-if="isDesktop"
							data-tauri-drag-region
							class="-ml-1 flex w-[calc(var(--sidebar-width)-1rem)] shrink-0 items-center group-has-[[data-collapsible=icon]]/sidebar-wrapper:mr-1 group-has-[[data-collapsible=icon]]/sidebar-wrapper:w-auto"
						>
							<WindowControls />
						</div>
						<SidebarTrigger class="-ml-1" />
						<AddTaskModalTrigger class="-ml-1" />

						<Separator orientation="vertical" class="mr-2 h-4" />

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
							class="ml-auto flex min-w-0 flex-1 items-center justify-end gap-2"
						></div>
						<div class="flex items-center gap-2">
							<button
								v-if="!isDesktop"
								class="flex h-8 w-8 items-center justify-center rounded-pill text-ink-subtle transition hover:bg-surface-hover hover:text-ink"
								:class="{
									'bg-surface-hover text-ink': store.state.aiPanelOpen,
								}"
								aria-label="Ask AI"
								title="Ask AI"
								@click="store.commit('toggleAiPanel')"
							>
								<Sparkles class="h-4 w-4" />
							</button>
							<ActiveCursorAgents v-if="!isDesktop" />
							<NotificationBell />
						</div>
					</div>
				</header>

				<div
					class="flex min-h-max flex-1 flex-col gap-4 max-sm:overflow-x-hidden"
				>
					<slot />
				</div>
			</SidebarInset>
			<AiAssistantPanel v-if="store.getters.isLoggedIn" />
		</SidebarMobileCloser>
	</SidebarProvider>

	<!-- Exit Workspace Confirm Dialog -->
	<Dialog v-if="isDesktop" v-model:open="showLocalDialog">
		<DialogContent class="max-w-md">
			<DialogHeader>
				<DialogTitle>New local workspace</DialogTitle>
				<DialogDescription>
					Stored only on this Mac in ~/.tmgr.dev/workspaces — it never reaches the
					server. Back it up by copying the folder.
				</DialogDescription>
			</DialogHeader>
			<form class="flex flex-col gap-3" @submit.prevent="createLocalWorkspaceFromDialog">
				<input
					v-model="localName"
					data-selectable
					autofocus
					class="h-9 rounded-md border border-input bg-background px-3 text-sm outline-none focus:ring-2 focus:ring-ring dark:border-input dark:bg-background"
					placeholder="Personal, Client under NDA…"
				/>
				<p v-if="localError" class="text-xs text-destructive">{{ localError }}</p>
				<DialogFooter>
					<button
						type="submit"
						class="h-9 rounded-md bg-primary px-4 text-sm font-medium text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
						:disabled="!localName.trim() || creatingLocal"
					>
						{{ creatingLocal ? 'Creating…' : 'Create' }}
					</button>
				</DialogFooter>
			</form>
		</DialogContent>
	</Dialog>

	<Confirm
		v-if="showExitConfirm"
		title="Leave workspace"
		:body="`Are you sure you want to leave '${workspaceToExit?.name}' workspace?`"
		@on-ok="confirmExitWorkspace"
		@on-cancel="cancelExitWorkspace"
	/>
</template>
