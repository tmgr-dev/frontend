<template>
	<nav
		data-testid="workspace-rail"
		aria-label="Workspaces"
		class="flex h-full w-14 shrink-0 flex-col items-center gap-2 border-r border-sidebar-border bg-sidebar py-2"
	>
		<div
			data-tour="nav.workspace-switcher"
			class="flex min-h-0 w-full flex-1 flex-col items-center gap-2"
		>
			<ul
				class="flex min-h-0 w-full flex-col items-center gap-2 overflow-y-auto py-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
			>
				<li
					v-for="workspace in workspaces"
					:key="workspace.id"
					class="relative flex w-full shrink-0 justify-center"
				>
					<span
						v-if="isActive(workspace)"
						aria-hidden="true"
						class="absolute left-0 top-1/2 h-6 w-1 -translate-y-1/2 rounded-r-full bg-sidebar-primary"
					/>
					<Tooltip>
						<TooltipTrigger as-child>
							<button
								type="button"
								data-testid="workspace-rail-chip"
								:data-workspace-code="workspace.code"
								:data-active="isActive(workspace) || undefined"
								:aria-current="isActive(workspace) ? 'true' : undefined"
								:aria-label="
									workspace.is_local
										? `${workspace.name} (Local)`
										: workspace.name
								"
								class="relative flex size-9 items-center justify-center rounded-lg text-xs font-semibold outline-none transition-colors"
								:class="
									isActive(workspace)
										? 'bg-sidebar-primary text-sidebar-primary-foreground'
										: 'bg-sidebar-accent text-sidebar-foreground hover:bg-sidebar-primary/15 dark:hover:bg-sidebar-primary/25'
								"
								@click="select(workspace)"
							>
								{{ initials(workspace.name) }}
								<span
									v-if="workspace.is_local"
									aria-hidden="true"
									data-testid="workspace-rail-local"
									class="absolute -bottom-0.5 -right-0.5 size-2.5 rounded-full bg-amber-500 ring-2 ring-sidebar dark:bg-amber-400"
								/>
							</button>
						</TooltipTrigger>
						<TooltipContent side="right" align="center">
							{{ workspace.name
							}}<span v-if="workspace.is_local" class="text-muted-foreground">
								· Local</span
							>
						</TooltipContent>
					</Tooltip>
				</li>
			</ul>

			<DropdownMenu>
				<DropdownMenuTrigger as-child>
					<button
						type="button"
						aria-label="Add workspace"
						data-testid="workspace-rail-add"
						class="flex size-9 shrink-0 items-center justify-center rounded-lg border border-dashed border-sidebar-border text-sidebar-foreground/70 outline-none transition-colors hover:bg-sidebar-accent hover:text-sidebar-foreground data-[state=open]:bg-sidebar-accent"
					>
						<Plus class="size-4" />
					</button>
				</DropdownMenuTrigger>
				<DropdownMenuContent
					class="min-w-56 rounded-lg"
					side="right"
					align="start"
					:side-offset="8"
				>
					<DropdownMenuItem
						class="cursor-pointer gap-2 p-2"
						data-testid="workspace-rail-add-workspace"
						@click="$router.push('/settings/workspaces?create')"
					>
						<div
							class="flex size-6 items-center justify-center rounded-md border bg-background"
						>
							<Plus class="size-4" />
						</div>
						<div class="font-medium text-muted-foreground">Add workspace</div>
					</DropdownMenuItem>
					<DropdownMenuItem
						v-if="isDesktop"
						class="cursor-pointer gap-2 p-2"
						data-testid="workspace-rail-add-local"
						@click="$emit('create-local')"
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
				</DropdownMenuContent>
			</DropdownMenu>
		</div>

		<slot name="footer" />
	</nav>
</template>

<script lang="ts">
	import type { Workspace } from '@/actions/tmgr/workspaces';
	import {
		DropdownMenu,
		DropdownMenuContent,
		DropdownMenuItem,
		DropdownMenuTrigger,
	} from '@/components/ui/dropdown-menu';
	import {
		Tooltip,
		TooltipContent,
		TooltipTrigger,
	} from '@/components/ui/tooltip';
	import { avatarInitials } from '@/utils/avatarInitials';
	import { HardDrive, Plus } from 'lucide-vue-next';
	import { defineComponent, type PropType } from 'vue';
	import { isActiveWorkspace } from './workspaceRail';

	export default defineComponent({
		name: 'WorkspaceRail',
		components: {
			DropdownMenu,
			DropdownMenuContent,
			DropdownMenuItem,
			DropdownMenuTrigger,
			HardDrive,
			Plus,
			Tooltip,
			TooltipContent,
			TooltipTrigger,
		},
		props: {
			workspaces: { type: Array as PropType<Workspace[]>, required: true },
			activeWorkspace: {
				type: Object as PropType<Workspace | null>,
				default: null,
			},
			isDesktop: { type: Boolean, default: false },
		},
		emits: {
			select: (_workspace: Workspace) => true,
			'create-local': () => true,
		},
		setup(props, { emit }) {
			const isActive = (workspace: Workspace) =>
				isActiveWorkspace(workspace, props.activeWorkspace);
			const select = (workspace: Workspace) => {
				if (!isActive(workspace)) emit('select', workspace);
			};
			return { isActive, select, initials: avatarInitials };
		},
	});
</script>
