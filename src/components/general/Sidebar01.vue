<script lang="ts">
export const description
	= 'A sidebar that collapses to icons.'
export const iframeHeight = '800px'
export const containerClass = 'w-full h-full'
</script>

<script setup lang=ts>
import {
	Avatar,
	AvatarFallback,
	AvatarImage,
} from '@/components/ui/avatar'

import {
	Breadcrumb,
	BreadcrumbItem,
	BreadcrumbLink,
	BreadcrumbList,
	BreadcrumbSeparator,
} from '@/components/ui/breadcrumb'

import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuGroup,
	DropdownMenuItem,
	DropdownMenuLabel,
	DropdownMenuSeparator,
	DropdownMenuShortcut,
	DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu'
import { Separator } from '@/components/ui/separator'
import {
	Sidebar,
	SidebarContent,
	SidebarFooter,
	SidebarGroup,
	SidebarGroupLabel,
	SidebarHeader,
	SidebarInset,
	SidebarMenu,
	SidebarMenuAction,
	SidebarMenuButton,
	SidebarMenuItem,
	SidebarProvider,
	SidebarRail,
	SidebarTrigger,
} from '@/components/ui/sidebar'
import {
	AudioWaveform,
	BadgeCheck,
	Bell,
	ChevronsUpDown,
	Command,
	Cable,
	Folder,
	Forward,
	GalleryVerticalEnd,
	LogOut,
	MoreHorizontal,
	SquareKanban,
	Plus,
	Settings2,
	Inbox,
	Package,
	PackageOpen,
	Sparkles,
	SquareTerminal,
	Trash2,
} from 'lucide-vue-next'
import { onBeforeMount, ref } from 'vue';
import { getWorkspaces, Workspace } from '@/actions/tmgr/workspaces.ts';
import { getUser, updateUserSettingsV2, User } from '@/actions/tmgr/user.ts';
import { Category, getTopCategories } from '@/actions/tmgr/categories.ts';
import DarkMode from '@/components/general/DarkMode.vue';
import store from '@/store';


const foldersMoreThanFour = ref<boolean>(false);
const user = ref<User>({} as User);
const folders = ref<Category[]>([]);
const workspaces = ref<Workspace[]>([]);

// This is sample data.
const data = {
	user: {
		name: user.value?.name,
		email: 'm@example.com',
		avatar: '/avatars/shadcn.jpg',
	},
	teams: [
		{
			name: 'Acme Inc',
			logo: GalleryVerticalEnd,
			plan: 'Enterprise',
		},
		{
			name: 'Acme Corp.',
			logo: AudioWaveform,
			plan: 'Startup',
		},
		{
			name: 'Evil Corp.',
			logo: Command,
			plan: 'Free',
		},
	],
	navMain: [
		{
			title: 'Tasks',
			url: '#',
			icon: SquareTerminal,
			isActive: true,
			items: [
				{
					title: 'List',
					url: '/',
				},
				{
					title: 'Board',
					url: '/board',
				},
				{
					title: 'Folders',
					url: '/projects-categories',
				},
			],
		},
		{
			title: 'Settings',
			url: '#',
			icon: Settings2,
			isActive: true,
			items: [
				{
					title: 'Profile',
					url: '/settings?tab=profile',
				},
				{
					title: 'Notifications',
					url: '/settings?tab=notification',
				},
				{
					title: 'Workspace Settings',
					url: 'settings?tab=workspace',
				},
				{
					title: 'Smart Devices',
					url: 'settings?tab=device',
				},
			],
		},
	],
	projects: [],
}

const activeWorkspace = ref(workspaces.value[0])

onBeforeMount(async () => {
	const [foldersData, userData, workspacesData] = await Promise.all([
		getTopCategories(),
		getUser(),
		getWorkspaces()
	]);

	foldersMoreThanFour.value = foldersData?.length > 4;
	folders.value = foldersData.slice(0, 4);
	user.value = userData;
	workspaces.value = workspacesData;
	activeWorkspace.value = workspaces.value?.find((w: Workspace) => w.id === parseInt(user.value?.settings.find(s => s.id === 5)?.value)) as Workspace;
})

const setActiveWorkspace = async (workspace: Workspace) => {
	activeWorkspace.value = workspace
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
		store.commit('rerenderApp');
	} catch (error) {
		console.error('Failed to update workspace:', error);
		// You might want to add error handling here
	}
}
</script>

<template>
	<SidebarProvider>
		<Sidebar collapsible="icon">
			<SidebarHeader>
				<SidebarMenu>
					<SidebarMenuItem>
						<DropdownMenu>
							<DropdownMenuTrigger as-child>
								<SidebarMenuButton
									size="lg"
									class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
								>
									<div class="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
										<component :is="activeWorkspace?.logo || PackageOpen" class="size-4" />
									</div>
									<div class="grid flex-1 text-left text-sm leading-tight">
										<span class="truncate font-semibold">{{ activeWorkspace?.name }}</span>
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
									class="gap-2 p-2"
									@click="setActiveWorkspace(workspace)"
								>
									<div class="flex size-6 items-center justify-center rounded-sm border">
										<component :is="workspace.id === activeWorkspace.id ? PackageOpen : Package" class="size-4 shrink-0" />
									</div>
									{{ workspace.name }}
									<DropdownMenuShortcut>⌘{{ index + 1 }}</DropdownMenuShortcut>
								</DropdownMenuItem>
								<DropdownMenuSeparator />
								<DropdownMenuItem class="gap-2 p-2 cursor-pointer" @click="$router.push('/settings?tab=workspace&add-workspace=open')">
									<div class="flex size-6 items-center justify-center rounded-md border bg-background">
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
					<SidebarGroupLabel>Folders</SidebarGroupLabel>
					<SidebarMenu>
						<SidebarMenuItem
							v-for="item in folders"
							:key="item.title"
						>
							<SidebarMenuButton as-child>
								<router-link :to="`/projects-categories/${item.id}/children`">
<!--									<component :is="item.name" />-->
									<span>{{ item.title }}</span>
								</router-link>
							</SidebarMenuButton>
							<DropdownMenu>
								<DropdownMenuTrigger as-child>
									<SidebarMenuAction show-on-hover>
										<MoreHorizontal />
										<span class="sr-only">More</span>
									</SidebarMenuAction>
								</DropdownMenuTrigger>
								<DropdownMenuContent class="w-48 rounded-lg" side="bottom" align="end">
									<DropdownMenuItem>
										<Folder class="text-muted-foreground" />
										<span>View Project</span>
									</DropdownMenuItem>
									<DropdownMenuItem>
										<Forward class="text-muted-foreground" />
										<span>Share Project</span>
									</DropdownMenuItem>
									<DropdownMenuSeparator />
									<DropdownMenuItem>
										<Trash2 class="text-muted-foreground" />
										<span>Delete Project</span>
									</DropdownMenuItem>
								</DropdownMenuContent>
							</DropdownMenu>
						</SidebarMenuItem>
						<SidebarMenuItem>
							<SidebarMenuButton class="text-sidebar-foreground/70" @click="$router.push('/projects-categories')">
								<MoreHorizontal class="text-sidebar-foreground/70" />
								<span>More</span>
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
										<AvatarImage :src="data.user.avatar" :alt="data.user.name" />
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
							<DropdownMenuContent class="w-[--radix-dropdown-menu-trigger-width] min-w-56 rounded-lg" side="bottom" align="end" :side-offset="4">
								<DropdownMenuLabel class="p-0 font-normal">
									<div class="flex items-center gap-2 px-1 py-1.5 text-left text-sm">
										<Avatar class="h-8 w-8 rounded-lg">
											<AvatarImage :src="data.user.avatar" :alt="data.user.name" />
											<AvatarFallback class="rounded-lg">
												{{ user?.name?.charAt(0).toUpperCase() }}
											</AvatarFallback>
										</Avatar>
										<div class="grid flex-1 text-left text-sm leading-tight">
											<span class="truncate font-semibold">{{ user.name }}</span>
											<span class="truncate text-xs">{{ user.email }}</span>
										</div>
									</div>
								</DropdownMenuLabel>
								<DropdownMenuSeparator />
								<DropdownMenuGroup>
									<DropdownMenuItem>
										<Sparkles />
										Upgrade to Pro
									</DropdownMenuItem>
								</DropdownMenuGroup>
								<DropdownMenuSeparator />
								<DropdownMenuGroup>
									<DropdownMenuItem @click="$router.push('/settings?tab=profile')" class="cursor-pointer">
										<BadgeCheck />
										Profile
									</DropdownMenuItem>
									<DropdownMenuItem @click="$router.push('/settings?tab=device')" class="cursor-pointer">
										<Cable />
										Smart Devices
									</DropdownMenuItem>
									<DropdownMenuItem @click="$router.push('/settings?tab=notification')" class="cursor-pointer">
										<Bell />
										Notifications
									</DropdownMenuItem>
									<DropdownMenuItem @click="$router.push('/settings?tab=workspace')" class="cursor-pointer">
										<Settings2 />
										Workspace Settings
									</DropdownMenuItem>
									<DropdownMenuItem>
										<DarkMode/>
									</DropdownMenuItem>
								</DropdownMenuGroup>
								<DropdownMenuSeparator />
								<DropdownMenuItem>
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
			<header class="flex h-16 shrink-0 items-center gap-2 transition-[width,height] ease-linear group-has-[[data-collapsible=icon]]/sidebar-wrapper:h-12">
				<div class="flex items-center gap-2 px-4">
					<SidebarTrigger class="-ml-1" />
					<Separator orientation="vertical" class="mr-2 h-4" />
					<Breadcrumb>
						<BreadcrumbList>
							<BreadcrumbItem class="hidden md:block">
								<BreadcrumbLink href="#">
									TMGR.DEV
								</BreadcrumbLink>
							</BreadcrumbItem>
							<BreadcrumbSeparator class="hidden md:block" />
							<BreadcrumbItem class="hidden md:block">
								<div id="breadcrumb"></div>
							</BreadcrumbItem>
						</BreadcrumbList>
					</Breadcrumb>
				</div>
			</header>
			<div class="flex flex-1 flex-col gap-4 p-4 pt-0">
<!--				<div class="grid auto-rows-min gap-4 md:grid-cols-3">-->
<!--					<div class="aspect-video rounded-xl bg-muted/50" />-->
<!--					<div class="aspect-video rounded-xl bg-muted/50" />-->
<!--					<div class="aspect-video rounded-xl bg-muted/50" />-->
<!--				</div>-->
				<div class="min-h-[100vh] flex-1 rounded-xl bg-muted/50 md:min-h-min">
					<slot></slot>
				</div>
			</div>
		</SidebarInset>
	</SidebarProvider>
</template>
