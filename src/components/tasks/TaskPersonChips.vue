<template>
	<div
		v-if="people.length"
		class="flex flex-wrap items-center gap-1.5"
		data-testid="task-person-chips"
	>
		<router-link
			v-for="person in people"
			:key="person.page_id"
			:to="`/${workspaceCode}/pages/${person.slug || person.page_id}`"
			class="inline-flex items-center gap-1 rounded-full bg-violet-100 px-2.5 py-0.5 text-xs font-medium text-violet-800 hover:underline dark:bg-violet-900/40 dark:text-violet-200"
		>
			<UserRound class="h-3 w-3" />
			{{ person.title }}
		</router-link>
	</div>
</template>

<script lang="ts">
	import store from '@/store';
	import { UserRound } from 'lucide-vue-next';
	import { computed, defineComponent, type PropType } from 'vue';

	export interface MentionedPerson {
		page_id: number;
		slug: string;
		title: string;
	}

	export default defineComponent({
		name: 'TaskPersonChips',
		components: { UserRound },
		props: {
			people: {
				type: Array as PropType<MentionedPerson[]>,
				default: () => [],
			},
		},
		setup() {
			const workspaceCode = computed(
				() => store.getters.currentWorkspace?.code ?? '',
			);
			return { workspaceCode };
		},
	});
</script>
