<template>
	<template v-if="entries.length">
		<DropdownMenuSub v-if="submenu">
			<DropdownMenuSubTrigger :data-testid="`${testIdPrefix}-submenu`">
				<Share2 class="mr-2 h-4 w-4" />
				<span>Share</span>
			</DropdownMenuSubTrigger>
			<DropdownMenuSubContent class="w-56">
				<DropdownMenuItem
					v-for="entry in entries"
					:key="entry.key"
					:data-testid="`${testIdPrefix}-${entry.key}`"
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
				:data-testid="`${testIdPrefix}-${entry.key}`"
				@click="entry.run"
			>
				<component :is="entry.icon" class="mr-2 h-4 w-4" />
				<span>{{ entry.label }}</span>
			</DropdownMenuItem>
		</template>
	</template>
</template>

<script lang="ts">
	import {
		DropdownMenuItem,
		DropdownMenuSub,
		DropdownMenuSubContent,
		DropdownMenuSubTrigger,
	} from '@/components/ui/dropdown-menu';
	import { toast } from '@/components/ui/toast';
	import { canOpenInApp, writeClipboard } from '@/utils/share';
	import { AppWindow, FileText, Link, Link2, Share2 } from 'lucide-vue-next';
	import { computed, defineComponent, type PropType } from 'vue';

	const TOASTS = {
		link: { copied: 'Link copied', failed: "Couldn't copy the link" },
		markdown: {
			copied: 'Markdown copied',
			failed: "Couldn't copy the Markdown",
		},
	};

	export default defineComponent({
		name: 'ShareMenuItems',
		components: {
			DropdownMenuItem,
			DropdownMenuSub,
			DropdownMenuSubContent,
			DropdownMenuSubTrigger,
			Share2,
		},
		props: {
			web: { type: String as PropType<string | null>, default: null },
			app: { type: String as PropType<string | null>, default: null },
			markdown: {
				type: Function as PropType<() => string | Promise<string>>,
				default: null,
			},
			markdownToast: {
				type: String as PropType<keyof typeof TOASTS>,
				default: 'link',
			},
			submenu: { type: Boolean, default: false },
			openInApp: { type: Boolean, default: false },
			testIdPrefix: { type: String, required: true },
		},
		setup(props) {
			const copy = (
				value: () => string | Promise<string>,
				kind: keyof typeof TOASTS,
			) => {
				let write: Promise<void>;
				try {
					write = writeClipboard(value());
				} catch (error) {
					write = Promise.reject(error);
				}
				write.then(
					() => toast({ title: TOASTS[kind].copied }),
					() => toast({ title: TOASTS[kind].failed, variant: 'destructive' }),
				);
			};

			const entries = computed(() => {
				const { web, app, markdown } = props;
				const list = [];
				if (web) {
					list.push({
						key: 'copy-link',
						label: 'Copy link',
						icon: Link,
						run: () => copy(() => web, 'link'),
					});
				}
				if (app) {
					list.push({
						key: 'copy-app-link',
						label: 'Copy app link',
						icon: Link2,
						run: () => copy(() => app, 'link'),
					});
				}
				if (markdown) {
					list.push({
						key: 'copy-markdown',
						label: 'Copy as Markdown',
						icon: FileText,
						run: () => copy(markdown, props.markdownToast),
					});
				}
				if (app && props.openInApp && canOpenInApp()) {
					list.push({
						key: 'open-app',
						label: 'Open in TMGR app',
						icon: AppWindow,
						run: () => {
							window.location.href = app;
						},
					});
				}
				return list;
			});

			return { entries };
		},
	});
</script>
