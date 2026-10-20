<template>
	<DropdownMenu>
		<DropdownMenuTrigger as-child>
			<button
				v-if="variant === 'rail'"
				type="button"
				aria-label="User menu"
				data-testid="workspace-rail-user"
				data-tour="nav.user-menu"
				class="flex size-10 items-center justify-center rounded-full outline-none ring-sidebar-ring transition hover:ring-2 focus-visible:ring-2 data-[state=open]:ring-2"
			>
				<UserAvatar
					:user-id="user?.id ?? 0"
					:name="user?.name ?? ''"
					:has-avatar="user?.has_avatar ?? false"
					:size="32"
				/>
			</button>
			<SidebarMenuButton
				v-else
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
						<span class="truncate font-semibold">{{ user.name }}</span>
						<span
							v-if="isLocalOnly"
							data-testid="local-badge"
							class="shrink-0 rounded bg-muted px-1.5 py-0.5 text-2xs font-medium text-muted-foreground"
							>Local</span
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
			class="min-w-56 rounded-lg"
			:class="{ 'w-[--radix-dropdown-menu-trigger-width]': variant !== 'rail' }"
			:side="variant === 'rail' ? 'right' : 'bottom'"
			align="end"
			:side-offset="4"
		>
			<DropdownMenuLabel class="p-0 font-normal">
				<div class="flex items-center gap-2 px-1 py-1.5 text-left text-sm">
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
					<BadgeCheck />
					Profile
				</DropdownMenuItem>
				<DropdownMenuItem
					@click="$router.push('/settings/sections')"
					class="cursor-pointer"
				>
					<Settings2 />
					Settings
				</DropdownMenuItem>
				<DropdownMenuItem
					v-if="workspaceCode && hubHidden"
					data-tour="nav.getting-started"
					@click="$router.push(`/${workspaceCode}/getting-started`)"
					class="cursor-pointer"
				>
					<LifeBuoy />
					Getting started
				</DropdownMenuItem>
				<DropdownMenuItem>
					<DarkMode />
				</DropdownMenuItem>
			</DropdownMenuGroup>

			<template v-if="!isLocalOnly">
				<DropdownMenuSeparator />

				<DropdownMenuItem class="cursor-pointer" @click="$emit('logout')">
					<LogOut />
					Log out
				</DropdownMenuItem>
			</template>
		</DropdownMenuContent>
	</DropdownMenu>
</template>

<script lang="ts">
	import type { User } from '@/actions/tmgr/user';
	import DarkMode from '@/components/general/DarkMode.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuGroup,
		DropdownMenuItem,
		DropdownMenuLabel,
		DropdownMenuSeparator,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import { SidebarMenuButton } from '@/components/ui/sidebar';
	import {
		BadgeCheck,
		ChevronsUpDown,
		LifeBuoy,
		LogOut,
		Settings2,
	} from 'lucide-vue-next';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'SidebarUserMenu',
		components: {
			BadgeCheck,
			ChevronsUpDown,
			DarkMode,
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuGroup,
			DropdownMenuItem,
			DropdownMenuLabel,
			DropdownMenuSeparator,
			DropdownMenuTrigger,
			LifeBuoy,
			LogOut,
			Settings2,
			SidebarMenuButton,
			UserAvatar,
		},
		props: {
			user: { type: Object as PropType<User>, required: true },
			variant: {
				type: String as PropType<'footer' | 'rail'>,
				default: 'footer',
			},
			isLocalOnly: { type: Boolean, default: false },
			hubHidden: { type: Boolean, default: false },
			workspaceCode: { type: String, default: '' },
		},
		emits: ['logout'],
	});
</script>
