import { computed } from 'vue';
import { useToast } from '@/components/ui/toast/use-toast';
import { currentTaskMenuItems } from '@/pluginSystem/taskMenuItems';
import { detachedTaskMenuClient } from '@/pluginSystem/taskMenuRelay';
import { pluginHost, pluginState } from '@/pluginSystem/state';
import { needsTaskMenuSubmenu, type TaskMenuItem } from '@/pluginSystem/taskMenu';
import { isInDetachedWindow } from '@/utils/taskWindow';

export const usePluginTaskMenu = () => {
	const { toast } = useToast();

	const items = computed<TaskMenuItem[]>(() =>
		isInDetachedWindow()
			? (detachedTaskMenuClient()?.items.value ?? [])
			: currentTaskMenuItems(),
	);
	const useSubmenu = computed(() => needsTaskMenuSubmenu(items.value));

	const run = async (item: TaskMenuItem, taskId: number) => {
		try {
			if (isInDetachedWindow()) {
				const client = detachedTaskMenuClient();
				if (!client) throw new Error('The plugin menu is not available');
				await client.run(item, taskId);
			} else {
				await pluginHost()?.runTaskMenuCommand(item.pluginId, item.command, taskId);
			}
		} catch (error) {
			if (pluginState.plugins[item.pluginId]?.status === 'crashed') return;
			toast({
				title: 'The plugin command failed',
				description: error instanceof Error ? error.message : String(error),
				variant: 'destructive',
			});
		}
	};

	return { items, useSubmenu, run };
};
