<template>
	<p
		v-show="context"
		class="text-xs"
		:class="levelClass"
		title="Context window usage"
	>
		{{ label }}
	</p>
</template>

<script lang="ts">
	import { getAgentContext } from '@/actions/tmgr/agent';
	import type { AgentContext } from '@/types/agent';
	import {
		contextLabel,
		contextLevel,
		contextPercent,
	} from '@/utils/contextMeter';
	import { createRequestSequence } from '@/utils/requestSequence';
	import {
		computed,
		defineComponent,
		onUnmounted,
		ref,
		watch,
		type PropType,
	} from 'vue';

	const LEVEL_CLASS = {
		normal: 'text-ink-subtle',
		warning: 'text-amber-600 dark:text-amber-400',
		danger: 'text-red-500',
	};

	export default defineComponent({
		name: 'ContextMeter',
		props: {
			conversationId: {
				type: Number as PropType<number | null>,
				default: null,
			},
			refreshKey: {
				type: Number,
				default: 0,
			},
		},
		setup(props) {
			const context = ref<AgentContext | null>(null);
			const requests = createRequestSequence();

			const label = computed(() =>
				context.value ? contextLabel(context.value) : '',
			);
			const levelClass = computed(() => {
				const percent = context.value
					? contextPercent(
							context.value.context_tokens,
							context.value.context_limit,
					  )
					: null;
				return LEVEL_CLASS[percent === null ? 'normal' : contextLevel(percent)];
			});

			const load = async (conversationId: number | null): Promise<void> => {
				const token = requests.begin();
				if (context.value?.conversation_id !== conversationId)
					context.value = null;
				if (conversationId === null) {
					context.value = null;
					return;
				}
				try {
					const result = await getAgentContext(conversationId);
					if (requests.isCurrent(token)) context.value = result;
				} catch {
					if (requests.isCurrent(token)) context.value = null;
				}
			};

			watch(
				() => [props.conversationId, props.refreshKey] as const,
				([conversationId]) => load(conversationId),
				{ immediate: true },
			);

			onUnmounted(() => requests.dispose());

			return { context, label, levelClass };
		},
	});
</script>
