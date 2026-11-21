<template>
	<div
		class="pop m"
		role="listbox"
		:aria-label="$t('manager.slash.label')"
		@mousedown.prevent
	>
		<template v-for="group in groups" :key="group.name">
			<div class="pt" role="presentation">{{ group.name }}</div>
			<button
				v-for="item in group.items"
				:id="`slash-opt-${item.index}`"
				:key="item.command.name"
				type="button"
				role="option"
				:aria-selected="item.index === activeIndex"
				:class="{ on: item.index === activeIndex }"
				@mouseenter="$emit('hover', item.index)"
				@click="$emit('select', item.command)"
			>
				<b class="mono">/{{ item.command.name.replace(/^\//, '') }}</b>
				<small
					>{{ item.command.description }}
					<span v-if="item.command.args" class="args mono">{{
						item.command.args
					}}</span>
					<span v-if="item.command.dangerous" class="ask">{{
						$t('manager.slash.dangerous')
					}}</span></small
				>
			</button>
		</template>
	</div>
</template>

<script lang="ts">
	import { defineComponent, type PropType } from 'vue';
	import type { SlashCommand } from '../types';

	export default defineComponent({
		name: 'ManagerSlashMenu',
		props: {
			commands: { type: Array as PropType<SlashCommand[]>, required: true },
			activeIndex: { type: Number, default: 0 },
		},
		emits: ['select', 'hover'],
		computed: {
			groups(): {
				name: string;
				items: { command: SlashCommand; index: number }[];
			}[] {
				const out: {
					name: string;
					items: { command: SlashCommand; index: number }[];
				}[] = [];
				this.commands.forEach((command, index) => {
					let g = out.find((x) => x.name === command.group);
					if (!g) {
						g = { name: command.group, items: [] };
						out.push(g);
					}
					g.items.push({ command, index });
				});
				return out;
			},
		},
	});
</script>
