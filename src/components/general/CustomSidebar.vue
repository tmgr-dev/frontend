<script setup lang="ts">
	import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';

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
		SidebarMenuButton,
		SidebarMenuItem,
		SidebarProvider,
		SidebarRail,
		SidebarTrigger,
	} from '@/components/ui/sidebar';
	import SidebarMobileCloser from '@/components/ui/sidebar/SidebarMobileCloser.vue';
	import {
		BadgeCheck,
		Bell,
		ChevronsUpDown,
		Cable,
		LogOut,
		SquareKanban,
		Plus,
		Settings2,
		Inbox,
		Package,
		PackageOpen,
		ArchiveIcon,
		FolderClosedIcon,
		ClipboardListIcon,
		LayoutDashboard,
		UserPlus,
		Sliders,
	} from 'lucide-vue-next';
	import { onBeforeMount, ref, computed, onMounted, watch } from 'vue';
	import { getWorkspaces, Workspace, exitWorkspace } from '@/actions/tmgr/workspaces.ts';
	import { getUser, updateUserSettingsV2, User, getUserSettings } from '@/actions/tmgr/user.ts';
	import { Category, getTopCategories } from '@/actions/tmgr/categories.ts';
	import DarkMode from '@/components/general/DarkMode.vue';
	import Confirm from '@/components/general/Confirm.vue';
	import store from '@/store';
	import { logout as logoutAction } from '@/actions/tmgr/auth.ts';
	import AddTaskModalTrigger from '@/components/ui/sidebar/AddTaskModalTrigger.vue';
	import NotificationBell from '@/components/notifications/NotificationBell.vue';
	import { useRoute, useRouter } from 'vue-router';
	import { generateWorkspaceUrl, generateCategoryUrl } from '@/utils/url';
	import { useFeatureToggles } from '@/composable/useFeatureToggles';

	const route = useRoute();
	const router = useRouter();
	const { isFeatureEnabled, isUserFeatureEnabled } = useFeatureToggles();
	const user = ref<User>({} as User);
	const categories = ref<Category[]>([]);
	const workspaces = ref<Workspace[]>([]);
	const showExitConfirm = ref(false);
	const workspaceToExit = ref<Workspace | null>(null);

	const data = {
		user: {
			name: user.value?.name,
			avatar: '/avatars/shadcn.jpg',
		},
	};

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

	onBeforeMount(async () => {
		if (store.getters.isLoggedIn) {
			try {
				const [loadedCategories, userData, workspacesData] = await Promise.all([
					getTopCategories(),
					getUser(),
					getWorkspaces(),
					store.dispatch('featureToggles/loadUserToggles'),
				]);

				categories.value = loadedCategories.slice(0, 4);
				user.value = userData;
				workspaces.value = workspacesData;
				const activeWorkspaceId = user.value?.settings.find(
					(s) => s.key === 'current_workspace',
				)?.value;
				activeWorkspace.value = workspaces.value?.find(
					(workspace: Workspace) => workspace.id == activeWorkspaceId,
				) as Workspace;
				
				// Load workspace feature toggles for the active workspace
				if (activeWorkspace.value?.id) {
					await store.dispatch('featureToggles/loadWorkspaceToggles', activeWorkspace.value.id);
				}
			} catch (e) {
				console.error(e);
			}
		}
	});

	// Watch for changes to the current workspace in the store
	watch(() => store.state.user?.settings, (newSettings) => {
		if (newSettings && workspaces.value.length > 0) {
			const currentWorkspaceId = newSettings.find(
				(setting: any) => setting.key === 'current_workspace'
			)?.value;
			
			if (currentWorkspaceId) {
				const newActiveWorkspace = workspaces.value.find(
					(workspace: Workspace) => Number(workspace.id) === Number(currentWorkspaceId)
				);
				
				if (newActiveWorkspace && activeWorkspace.value?.id !== newActiveWorkspace.id) {
					console.log(`Updating active workspace in sidebar from ${activeWorkspace.value?.name} to ${newActiveWorkspace.name}`);
					activeWorkspace.value = newActiveWorkspace;
				}
			}
		}
	}, { deep: true });

	// Also watch for app rerenders
	watch(() => store.state.appRerenderKey, async () => {
		if (store.getters.isLoggedIn && workspaces.value.length > 0) {
			// Re-check the current workspace from settings
			const currentWorkspaceId = store.state.user?.settings?.find(
				(setting: any) => setting.key === 'current_workspace'
			)?.value;
			
			if (currentWorkspaceId) {
				const newActiveWorkspace = workspaces.value.find(
					(workspace: Workspace) => Number(workspace.id) === Number(currentWorkspaceId)
				);
				
				if (newActiveWorkspace) {
					activeWorkspace.value = newActiveWorkspace;
				}
			}
		}
	});

	const setActiveWorkspace = async (workspace: Workspace) => {
		// Update UI immediately
		activeWorkspace.value = workspace;
		
		try {
			// Update the URL to match the new workspace code
			const currentPath = window.location.pathname;
			const pathParts = currentPath.split('/');
			
			// If the URL starts with a workspace code, replace it
			if (pathParts.length > 1 && pathParts[1]) {
				// Replace the workspace code in the URL
				pathParts[1] = workspace.code;
				const newPath = pathParts.join('/');
				
				// Use history.replaceState to update the URL without navigating
				history.replaceState({}, '', newPath);
			}
			
			// Prepare settings update for backend
			const settingsWithUpdatedWorkspace = user.value?.settings.map(
				(setting) => {
					if (setting.key === 'current_workspace') {
						setting.value = workspace.id;
					}

					return {
						id: setting.id,
						value: setting.value,
					};
				},
			);

			// Update settings in backend
			await updateUserSettingsV2(settingsWithUpdatedWorkspace);
			
			// Update store without a page reload
			store.commit('setUser', {
				...store.state.user,
				settings: settingsWithUpdatedWorkspace
			});
			
			// Update the workspace setting directly
			store.commit('updateUserWorkspaceSetting', {
				workspaceId: workspace.id
			});
			
			// Load feature toggles for new workspace
			await store.dispatch('featureToggles/loadWorkspaceToggles', workspace.id);
			
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

	const canLeaveWorkspace = (workspace: Workspace) => {
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
			(setting: any) => setting.key === 'current_workspace'
		)?.value;
		
		return store.state.workspaces.find(
			(workspace: any) => Number(workspace.id) === Number(currentWorkspaceId)
		);
	});

	const archiveUrl = computed(() => generateWorkspaceUrl('archive', activeWorkspace.value));
</script>

<template>
	<SidebarProvider>
		<SidebarMobileCloser>
			<Sidebar collapsible="icon" v-if="store.getters.isLoggedIn">
			<SidebarHeader>
				<SidebarMenu>
					<SidebarMenuItem>
						<DropdownMenu>
							<DropdownMenuTrigger as-child>
								<SidebarMenuButton
									size="lg"
									class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
								>
									<div
										class="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground"
									>
										<PackageOpen class="size-4" />
									</div>

									<div class="grid flex-1 text-left text-sm leading-tight">
										<span class="truncate font-semibold">
											{{ activeWorkspace?.name }}
										</span>
										<span class="truncate text-xs">current workspace</span>
									</div>
									<ChevronsUpDown class="ml-auto" />
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

									<button
										v-if="canLeaveWorkspace(workspace)"
										@click.stop="handleExitWorkspace(workspace, $event)"
										class="ml-auto flex size-6 items-center justify-center rounded hover:bg-destructive/10 hover:text-destructive transition-colors"
										title="Leave workspace"
									>
										<LogOut class="size-4" />
									</button>
								</DropdownMenuItem>

								<DropdownMenuSeparator />

								<DropdownMenuItem
									class="cursor-pointer gap-2 p-2"
									@click="
										$router.push('/settings/workspaces?create')
									"
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

								<DropdownMenuItem
									class="cursor-pointer gap-2 p-2"
									@click="
										$router.push('/settings/workspaces?invite')
									"
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
								<router-link :to="activeWorkspace?.code 
									? `/${activeWorkspace.code}/dashboard` 
									: '/'">
									<LayoutDashboard />
									<span>Dashboard</span>
								</router-link>
							</SidebarMenuButton>
						</SidebarMenuItem>

						<SidebarMenuItem>
							<SidebarMenuButton as-child>
								<router-link :to="activeWorkspace?.code 
									? `/${activeWorkspace.code}/list` 
									: '/list'">
									<ClipboardListIcon />
									<span>List</span>
								</router-link>
							</SidebarMenuButton>
						</SidebarMenuItem>

						<SidebarMenuItem v-if="isFeatureEnabled('board')">
							<SidebarMenuButton as-child>
								<router-link :to="activeWorkspace?.code 
									? `/${activeWorkspace.code}/board` 
									: '/board'">
									<SquareKanban />
									<span>Board</span>
								</router-link>
							</SidebarMenuButton>
						</SidebarMenuItem>

						<SidebarMenuItem v-if="isFeatureEnabled('daily_routines')">
							<SidebarMenuButton as-child>
								<router-link to="/routines">
									<Inbox />
									<span>Daily Routines</span>
								</router-link>
							</SidebarMenuButton>
						</SidebarMenuItem>

						<SidebarMenuItem v-if="isFeatureEnabled('categories')">
							<SidebarMenuButton as-child>
								<router-link :to="activeWorkspace?.code 
									? `/${activeWorkspace.code}/categories` 
									: '/projects-categories'">
									<FolderClosedIcon />
									<span>Categories</span>
								</router-link>
							</SidebarMenuButton>
						</SidebarMenuItem>
					</SidebarMenu>
				</SidebarGroup>

				<SidebarGroup v-if="isFeatureEnabled('categories')" class="group-data-[collapsible=icon]:hidden">
					<SidebarGroupLabel>Recent categories</SidebarGroupLabel>

					<SidebarMenu>
						<SidebarMenuItem v-for="item in categories" :key="item.title">
							<SidebarMenuButton as-child>
								<router-link :to="activeWorkspace?.code ? generateCategoryUrl(item.id, activeWorkspace) : `/projects-categories/${item.id}/children`">
									<span>{{ item.title }}</span>
								</router-link>
							</SidebarMenuButton>
						</SidebarMenuItem>
					</SidebarMenu>
				</SidebarGroup>

				<SidebarGroup>
					<SidebarGroupLabel>More</SidebarGroupLabel>

					<SidebarMenu>
						<SidebarMenuItem>
							<SidebarMenuButton as-child>
								<router-link :to="activeWorkspace?.code ? `/${activeWorkspace.code}/archive` : '/archive'">
									<ArchiveIcon />
									<span>Archive</span>
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
									<Avatar class="h-8 w-8 rounded-lg">
										<AvatarImage
											:src="data.user.avatar"
											:alt="data.user.name"
										/>
										<AvatarFallback class="rounded-lg">
											{{ user?.name?.charAt(0).toUpperCase() }}
										</AvatarFallback>
									</Avatar>
									<div class="grid flex-1 text-left text-sm leading-tight">
										<span class="truncate font-semibold">{{ user.name }}</span>
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
										<Avatar class="h-8 w-8 rounded-lg">
											<AvatarImage
												:src="data.user.avatar"
												:alt="data.user.name"
											/>
											<AvatarFallback class="rounded-lg">
												{{ user?.name?.charAt(0).toUpperCase() }}
											</AvatarFallback>
										</Avatar>
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

			<SidebarInset>
				<header
					v-if="store.getters.isLoggedIn"
					class="flex h-16 shrink-0 items-center gap-2 transition-[width,height] ease-linear group-has-[[data-collapsible=icon]]/sidebar-wrapper:h-12"
				>
					<div class="flex items-center gap-2 px-4 flex-1">
						<SidebarTrigger class="-ml-1" />
						<AddTaskModalTrigger class="-ml-1" />

						<Separator orientation="vertical" class="mr-2 h-4" />

						<Breadcrumb>
							<BreadcrumbList>
								<BreadcrumbItem class="hidden md:block">
									<BreadcrumbLink>
										<router-link :to="activeWorkspace?.code ? `/${activeWorkspace.code}/list` : '/list'"> 
											TMGR.DEV 
										</router-link>
									</BreadcrumbLink>
								</BreadcrumbItem>

								<BreadcrumbSeparator class="hidden md:block" />

								<BreadcrumbItem class="hidden md:block">
									{{ store.state.metaTitle || route.meta.title }}
								</BreadcrumbItem>
							</BreadcrumbList>
						</Breadcrumb>

						<div class="ml-auto flex items-center gap-2">
							<NotificationBell />
						</div>
					</div>
				</header>

				<div class="flex min-h-max flex-1 flex-col gap-4 max-sm:overflow-x-hidden">
					<slot />
				</div>
			</SidebarInset>
		</SidebarMobileCloser>
	</SidebarProvider>

	<!-- Exit Workspace Confirm Dialog -->
	<Confirm
		v-if="showExitConfirm"
		title="Leave workspace"
		:body="`Are you sure you want to leave '${workspaceToExit?.name}' workspace?`"
		@on-ok="confirmExitWorkspace"
		@on-cancel="cancelExitWorkspace"
	/>
</template>
