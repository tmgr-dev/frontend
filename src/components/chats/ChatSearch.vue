<template>
	<div class="relative">
		<Input
			v-model="query"
			:placeholder="$t('chats.search.placeholder')"
			data-testid="chat-search"
			@input="schedule"
		/>
		<div
			v-if="query.trim()"
			class="absolute top-full right-0 left-0 z-10 mt-1 max-h-72 overflow-y-auto rounded-card border border-line bg-surface shadow-md"
		>
			<p v-if="searching" class="px-3 py-2 text-sm text-ink-subtle">
				{{ $t('common.states.loading') }}
			</p>
			<p
				v-else-if="!visibleHits.length"
				class="px-3 py-2 text-sm text-ink-subtle"
			>
				{{ $t('chats.search.empty') }}
			</p>
			<button
				v-for="hit in visibleHits"
				:key="hit.message_id"
				type="button"
				class="block w-full px-3 py-2 text-left hover:bg-surface-hover"
				@click="open(hit)"
			>
				<p class="truncate text-xs text-ink-subtle">
					{{ hit.chat_title || hit.author.name }}
				</p>
				<p class="truncate text-sm text-ink">{{ hit.snippet }}</p>
			</button>
		</div>
	</div>
</template>

<script lang="ts">
	import { searchChats } from '@/actions/tmgr/chats';
	import type { ChatMessageHit } from '@/actions/tmgr/chats';
	import { Input } from '@/components/ui/input';
	import { readableHits, useChats } from '@/composable/useChats';
	import { computed, defineComponent, onBeforeUnmount, ref, watch } from 'vue';

	export default defineComponent({
		name: 'ChatSearch',
		components: { Input },
		props: {
			workspaceId: { type: Number, required: true },
		},
		emits: ['open'],
		setup(props, { emit }) {
			const query = ref('');
			const hits = ref<ChatMessageHit[]>([]);
			const searching = ref(false);
			const { chats } = useChats();
			const visibleHits = computed(() => readableHits(hits.value, chats.value));
			let timer: ReturnType<typeof setTimeout> | null = null;
			let request = 0;

			const run = async () => {
				const q = query.value.trim();
				if (!q) return;
				const current = ++request;
				try {
					const found = await searchChats(props.workspaceId, q);
					if (current === request) hits.value = found;
				} catch {
					if (current === request) hits.value = [];
				} finally {
					if (current === request) searching.value = false;
				}
			};

			watch(
				() => props.workspaceId,
				() => {
					request++;
					query.value = '';
					hits.value = [];
					searching.value = false;
				},
			);

			const schedule = () => {
				if (timer) clearTimeout(timer);
				searching.value = true;
				timer = setTimeout(run, 300);
			};

			const open = (hit: ChatMessageHit) => {
				query.value = '';
				hits.value = [];
				emit('open', hit.chat_id);
			};

			onBeforeUnmount(() => {
				if (timer) clearTimeout(timer);
			});

			return { query, visibleHits, searching, schedule, open };
		},
	});
</script>
