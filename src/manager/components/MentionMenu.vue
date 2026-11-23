<template>
	<div
		class="pop m"
		role="listbox"
		:aria-label="$t('manager.mention.menuLabel')"
		@mousedown.prevent
	>
		<button
			v-for="(item, index) in items"
			:id="`mention-opt-${index}`"
			:key="item.key"
			type="button"
			role="option"
			:aria-selected="index === activeIndex"
			:aria-disabled="!item.ok"
			:class="{ on: index === activeIndex, off: !item.ok }"
			@mouseenter="item.ok && $emit('hover', index)"
			@click="item.ok && $emit('select', item)"
		>
			<b class="mono">@{{ item.handle }}</b>
			<small>
				<template v-if="item.ok">
					<span v-if="item.agent">{{ item.agent }}</span>
					<span v-if="item.ambiguous || item.agent" class="mono">{{
						item.sessionId
					}}</span>
					<span v-if="item.risky" class="ask">{{
						$t('manager.mention.needsConfirmation')
					}}</span>
				</template>
				<template v-else>{{
					item.reason || $t('manager.mention.unavailable')
				}}</template>
			</small>
		</button>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { MentionItem } from '../mentions';

	export default defineComponent({
		name: 'ManagerMentionMenu',
		props: {
			items: { type: Array as PropType<MentionItem[]>, required: true },
			activeIndex: { type: Number, default: 0 },
		},
		emits: ['select', 'hover'],
	});
</script>
