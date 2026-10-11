<template>
	<template v-if="entries.length">
		<DropdownMenuSub v-if="submenu">
			<DropdownMenuSubTrigger data-testid="task-share-submenu">
				<Share2 class="mr-2 h-4 w-4" />
				<span>Share</span>
			</DropdownMenuSubTrigger>
			<DropdownMenuSubContent class="w-56">
				<DropdownMenuItem
					v-for="entry in entries"
					:key="entry.key"
					:data-testid="`task-share-${entry.key}`"
					@click="entry.run"
				>
					<component :is="entry.icon" class="mr-2 h-4 w-4" />
					<span>{{ entry.label }}</span>
				</DropdownMenuItem>
			</DropdownMenuSubContent>
		</DropdownMenuSub>
		<template v-else>
			<DropdownMenuItem
				v-for="entry in entries"
				:key="entry.key"
				:data-testid="`task-share-${entry.key}`"
				@click="entry.run"
			>
				<component :is="entry.icon" class="mr-2 h-4 w-4" />
				<span>{{ entry.label }}</span>
			</DropdownMenuItem>
		</template>
	</template>
</template>

<script>
	import {
		DropdownMenuItem,
		DropdownMenuSub,
		DropdownMenuSubContent,
		DropdownMenuSubTrigger,
	} from '@/components/ui/dropdown-menu';
	import { toast } from '@/components/ui/toast';
	import { useCopyToClipboard } from '@/composable/useCopyToClipboard';
	import store from '@/store';
	import { isDesktopApp } from '@/utils/desktop';
	import { taskShareLinks } from '@/utils/taskLinks';
	import { AppWindow, Link, Link2, Share2, FileText } from 'lucide-vue-next';
	import { computed, defineComponent } from 'vue';

	const MOBILE_UA = /Android|iPhone|iPad|iPod/i;

	export default defineComponent({
		name: 'TaskShareItems',
		components: {
			DropdownMenuItem,
			DropdownMenuSub,
			DropdownMenuSubContent,
			DropdownMenuSubTrigger,
			Share2,
		},
		props: {
			task: { type: Object, required: true },
			submenu: { type: Boolean, default: false },
			openInApp: { type: Boolean, default: false },
		},
		setup(props) {
			const [copy] = useCopyToClipboard();

			const workspace = computed(
				() =>
					(store.state.workspaces || []).find(
						(w) =>
							props.task.workspace_id != null &&
							Number(w.id) === Number(props.task.workspace_id),
					) ?? store.getters.currentWorkspace,
			);

			const links = computed(() =>
				props.task.id && workspace.value?.code
					? taskShareLinks(props.task, workspace.value)
					: null,
			);

			const copyText = async (text) => {
				const ok = await copy(text);
				toast(
					ok
						? { title: 'Link copied' }
						: { title: "Couldn't copy the link", variant: 'destructive' },
				);
			};

			const entries = computed(() => {
				const value = links.value;
				if (!value) return [];
				const list = [];
				if (value.web) {
					list.push({
						key: 'copy-link',
						label: 'Copy link',
						icon: Link,
						run: () => copyText(value.web),
					});
				}
				list.push({
					key: 'copy-app-link',
					label: 'Copy app link',
					icon: Link2,
					run: () => copyText(value.app),
				});
				list.push({
					key: 'copy-markdown',
					label: 'Copy as Markdown',
					icon: FileText,
					run: () => copyText(value.markdown),
				});
				if (
					props.openInApp &&
					!isDesktopApp() &&
					!MOBILE_UA.test(navigator.userAgent)
				) {
					list.push({
						key: 'open-app',
						label: 'Open in TMGR app',
						icon: AppWindow,
						run: () => {
							window.location.href = value.app;
						},
					});
				}
				return list;
			});

			return { entries };
		},
	});
</script>
