<template>
	<div class="space-y-2">
		<Input
			v-model="filter"
			:placeholder="$t('chats.members.filter')"
			data-testid="chat-member-filter"
		/>
		<ul
			class="divide-y divide-line max-h-56 overflow-y-auto rounded-card border border-line bg-surface"
		>
			<li v-for="member in visible" :key="member.id">
				<label
					class="flex cursor-pointer items-center gap-2 px-3 py-2 text-sm text-ink hover:bg-surface-hover"
				>
					<input
						type="checkbox"
						:checked="modelValue.includes(member.id)"
						@change="toggle(member.id)"
					/>
					{{ member.name }}
				</label>
			</li>
			<li v-if="!visible.length" class="px-3 py-2 text-sm text-ink-subtle">
				{{ $t('chats.members.none') }}
			</li>
		</ul>
	</div>
</template>

<script lang="ts">
	import { Input } from '@/components/ui/input';
	import { computed, defineComponent, ref, type PropType } from 'vue';

	interface Member {
		id: number;
		name: string;
	}

	export default defineComponent({
		name: 'MemberPicker',
		components: { Input },
		props: {
			members: { type: Array as PropType<Member[]>, required: true },
			modelValue: { type: Array as PropType<number[]>, required: true },
		},
		emits: ['update:modelValue'],
		setup(props, { emit }) {
			const filter = ref('');
			const visible = computed(() => {
				const q = filter.value.trim().toLowerCase();
				return q
					? props.members.filter((m) => m.name.toLowerCase().includes(q))
					: props.members;
			});
			const toggle = (id: number) =>
				emit(
					'update:modelValue',
					props.modelValue.includes(id)
						? props.modelValue.filter((x) => x !== id)
						: [...props.modelValue, id],
				);
			return { filter, visible, toggle };
		},
	});
</script>
