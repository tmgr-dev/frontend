<template>
	<div
		v-if="refs.length"
		class="flex flex-wrap items-center gap-1.5"
		data-testid="task-link-chips"
	>
		<span class="text-2xs font-bold uppercase tracking-wide text-ink-subtle"
			>Ссылки</span
		>
		<button
			v-for="ref in refs"
			:key="`${ref.kind}/${ref.id}`"
			type="button"
			class="inline-flex max-w-[16rem] items-center gap-1 truncate rounded-full bg-blue-100 px-2.5 py-0.5 text-xs text-blue-800 hover:underline dark:bg-blue-900/40 dark:text-blue-200"
			@click="open(ref)"
		>
			{{ label(ref) }}
		</button>
	</div>
</template>

<script lang="ts">
	import { useTmgrDirectory } from '@/components/pages/useTmgrDirectory';
	import store from '@/store';
	import {
		extractTaskLinkRefs,
		routeForTmgr,
		type TaskLinkRef,
	} from '@/utils/pages/taskLinks';
	import { computed, defineComponent, type PropType, watch } from 'vue';
	import { useRouter } from 'vue-router';

	export default defineComponent({
		name: 'TaskLinkChips',
		props: {
			sources: {
				type: Array as PropType<(string | object | null | undefined)[]>,
				default: () => [],
			},
		},
		setup(props) {
			const router = useRouter();
			const workspaceCode = computed(
				() => store.getters.currentWorkspace?.code ?? '',
			);
			const directory = useTmgrDirectory(() => workspaceCode.value);
			const refs = computed(() => extractTaskLinkRefs(...props.sources));

			watch(
				refs,
				(list) => {
					if (list.length) void directory.ensure(list);
				},
				{ immediate: true },
			);

			const label = (ref: TaskLinkRef) =>
				directory.titleFor(ref.kind, ref.id) ??
				(ref.kind === 'task' ? `#${ref.id}` : `Страница ${ref.id}`);

			const open = (ref: TaskLinkRef) => {
				const path = routeForTmgr(
					{ form: 'storage', kind: ref.kind, id: ref.id },
					workspaceCode.value,
				);
				if (path) void router.push(path);
			};

			return { refs, label, open };
		},
	});
</script>
