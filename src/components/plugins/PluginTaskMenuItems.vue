<script setup lang="ts">
	import {
		DropdownMenuItem,
		DropdownMenuSeparator,
		DropdownMenuSub,
		DropdownMenuSubContent,
		DropdownMenuSubTrigger,
	} from '@/components/ui/dropdown-menu';
	import { usePluginTaskMenu } from '@/composable/usePluginTaskMenu';

	withDefaults(defineProps<{ taskId: number; separator?: boolean }>(), {
		separator: true,
	});

	const { items, useSubmenu, run } = usePluginTaskMenu();
</script>

<template>
	<template v-if="items.length">
		<DropdownMenuSeparator v-if="separator" data-testid="plugin-task-menu-separator" />
		<DropdownMenuSub v-if="useSubmenu">
			<DropdownMenuSubTrigger data-testid="plugin-task-menu-submenu">
				Plugins
			</DropdownMenuSubTrigger>
			<DropdownMenuSubContent class="w-56">
				<DropdownMenuItem
					v-for="item in items"
					:key="`${item.pluginId}:${item.command}`"
					data-testid="plugin-task-menu-item"
					:title="item.pluginName"
					@click="run(item, taskId)"
				>
					<span class="min-w-0 flex-1 truncate">{{ item.title }}</span>
					<span class="shrink-0 text-2xs text-ink-faint">{{ item.pluginName }}</span>
				</DropdownMenuItem>
			</DropdownMenuSubContent>
		</DropdownMenuSub>
		<template v-else>
			<DropdownMenuItem
				v-for="item in items"
				:key="`${item.pluginId}:${item.command}`"
				data-testid="plugin-task-menu-item"
				:title="item.pluginName"
				@click="run(item, taskId)"
			>
				<span class="min-w-0 flex-1 truncate">{{ item.title }}</span>
				<span class="shrink-0 text-2xs text-ink-faint">{{ item.pluginName }}</span>
			</DropdownMenuItem>
		</template>
	</template>
</template>
