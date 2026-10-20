<template>
	<ShareMenuItems
		:web="links.web"
		:app="links.app"
		:markdown="markdown"
		markdown-toast="markdown"
		:submenu="submenu"
		:open-in-app="openInApp"
		test-id-prefix="page-share"
	/>
</template>

<script lang="ts">
	import { getPage, type Page, type PageSummary } from '@/actions/tmgr/pages';
	import ShareMenuItems from '@/components/share/ShareMenuItems.vue';
	import store from '@/store';
	import {
		loadPageExporter,
		pageShareLinks,
		pageShareMarkdown,
	} from '@/utils/pages/shareLinks';
	import { computed, defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'PageShareItems',
		components: { ShareMenuItems },
		props: {
			page: {
				type: Object as PropType<PageSummary | Page>,
				required: true,
			},
			loaded: { type: Boolean, default: false },
			workspaceCode: { type: String, required: true },
			submenu: { type: Boolean, default: false },
			openInApp: { type: Boolean, default: false },
		},
		setup(props) {
			void loadPageExporter().catch(() => undefined);

			const links = computed(() => {
				const workspace =
					(store.state.workspaces || []).find(
						(w: { code: string }) => w.code === props.workspaceCode,
					) ?? store.getters.currentWorkspace;
				return workspace
					? pageShareLinks(props.page, {
							id: workspace.id,
							code: props.workspaceCode,
					  })
					: { web: null, app: null };
			});

			const markdown = () =>
				props.loaded
					? pageShareMarkdown(props.page as Page, props.workspaceCode)
					: getPage(props.page.id).then((page) =>
							pageShareMarkdown(page, props.workspaceCode),
					  );

			return { links, markdown };
		},
	});
</script>
