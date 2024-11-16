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
		DropdownMenuShortcut,
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
	import {
		BadgeCheck,
		Bell,
		ChevronsUpDown,
		Cable,
		LogOut,
		MoreHorizontal,
		SquareKanban,
		Plus,
		Settings2,
		Inbox,
		Package,
		PackageOpen,
		SquareTerminal,
	} from 'lucide-vue-next';
	import { onBeforeMount, ref } from 'vue';
	import { getWorkspaces, Workspace } from '@/actions/tmgr/workspaces.ts';
	import { getUser, updateUserSettingsV2, User } from '@/actions/tmgr/user.ts';
	import { Category, getTopCategories } from '@/actions/tmgr/categories.ts';
	import DarkMode from '@/components/general/DarkMode.vue';
	import store from '@/store';
	import { logout as logoutAction } from '@/actions/tmgr/auth.ts';
	import AddTaskModalTrigger from '@/components/ui/sidebar/AddTaskModalTrigger.vue';
	import { useRoute } from 'vue-router';

	const route = useRoute();
	const user = ref<User>({} as User);
	const categories = ref<Category[]>([]);
	const workspaces = ref<Workspace[]>([]);

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
		} catch (e) {
			console.error(e);
		}
	}

	const activeWorkspace = ref(workspaces.value[0]);

	onBeforeMount(async () => {
		if (store.getters.isLoggedIn) {
			const [loadedCategories, userData, workspacesData] = await Promise.all([
				getTopCategories(),
				getUser(),
				getWorkspaces(),
			]);

			categories.value = loadedCategories.slice(0, 4);
			user.value = userData;
			workspaces.value = workspacesData;
			activeWorkspace.value = workspaces.value?.find(
				(w: Workspace) =>
					w.id ===
					parseInt(user.value?.settings.find((s) => s.id === 5)?.value),
			) as Workspace;
		}
	});

	const setActiveWorkspace = async (workspace: Workspace) => {
		activeWorkspace.value = workspace;
		try {
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

			await updateUserSettingsV2(settingsWithUpdatedWorkspace);
			document.location.reload();
		} catch (error) {
			console.error('Failed to update workspace:', error);
		}
	};
</script>

<template>
	<SidebarProvider>
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
										<component
											:is="activeWorkspace?.logo || PackageOpen"
											class="size-4"
										/>
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
								class="w-[--radix-dropdown-menu-trigger-width] min-w-56 rounded-lg"
								align="start"
								side="bottom"
								:side-offset="4"
							>
								<DropdownMenuLabel class="text-xs text-muted-foreground">
									Workspaces
								</DropdownMenuLabel>

								<DropdownMenuItem
									v-for="(workspace, index) in workspaces"
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
									{{ workspace.name }}
									<DropdownMenuShortcut>⌘{{ index + 1 }}</DropdownMenuShortcut>
								</DropdownMenuItem>

								<DropdownMenuSeparator />

								<DropdownMenuItem
									class="cursor-pointer gap-2 p-2"
									@click="
										$router.push('/settings?tab=workspace&add-workspace=open')
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
							</DropdownMenuContent>
						</DropdownMenu>
					</SidebarMenuItem>
				</SidebarMenu>
			</SidebarHeader>

			<SidebarContent>
				<SidebarGroup>
					<SidebarGroupLabel>TMGR.DEV</SidebarGroupLabel>

					<SidebarMenu>
						<SidebarMenuItem>
							<SidebarMenuButton as-child>
								<router-link to="/">
									<component :is="SquareTerminal" />
									<span>List</span>
								</router-link>
							</SidebarMenuButton>
						</SidebarMenuItem>

						<SidebarMenuItem>
							<SidebarMenuButton as-child>
								<router-link to="/board">
									<component :is="SquareKanban" />
									<span>Board</span>
								</router-link>
							</SidebarMenuButton>
						</SidebarMenuItem>

						<SidebarMenuItem>
							<SidebarMenuButton as-child>
								<router-link to="/daily-routines">
									<component :is="Inbox" />
									<span>Daily Routines</span>
								</router-link>
							</SidebarMenuButton>
						</SidebarMenuItem>
					</SidebarMenu>
				</SidebarGroup>

				<SidebarGroup class="group-data-[collapsible=icon]:hidden">
					<SidebarGroupLabel>Categories</SidebarGroupLabel>

					<SidebarMenu>
						<SidebarMenuItem v-for="item in categories" :key="item.title">
							<SidebarMenuButton as-child>
								<router-link :to="`/projects-categories/${item.id}/children`">
									<span>{{ item.title }}</span>
								</router-link>
							</SidebarMenuButton>
						</SidebarMenuItem>

						<SidebarMenuItem>
							<SidebarMenuButton
								class="text-sidebar-foreground/70"
								@click="$router.push('/projects-categories')"
							>
								<MoreHorizontal class="text-sidebar-foreground/70" />
								<span>More categories</span>
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
										@click="$router.push('/settings?tab=workspace')"
										class="cursor-pointer"
									>
										<Settings2 />
										Workspace Settings
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
				<div class="flex items-center gap-2 px-4">
					<SidebarTrigger class="-ml-1" />
					<AddTaskModalTrigger class="-ml-1" />

					<Separator orientation="vertical" class="mr-2 h-4" />

					<Breadcrumb>
						<BreadcrumbList>
							<BreadcrumbItem class="hidden md:block">
								<BreadcrumbLink>
									<router-link to="/"> TMGR.DEV </router-link>
								</BreadcrumbLink>
							</BreadcrumbItem>

							<BreadcrumbSeparator class="hidden md:block" />

							<BreadcrumbItem class="hidden md:block">
								{{ route.meta.title || store.state.metaTitle }}
							</BreadcrumbItem>
						</BreadcrumbList>
					</Breadcrumb>
				</div>
			</header>

			<div class="flex flex-1 flex-col gap-4">
				<slot></slot>
			</div>
		</SidebarInset>
	</SidebarProvider>
</template>
