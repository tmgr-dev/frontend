<template>
	<CommandGroup v-if="personas.length" heading="Personas">
		<CommandItem
			v-for="persona in personas"
			:key="persona.id"
			:value="`persona ${persona.name} ${persona.owner.name} ${persona.id}`"
			class="cursor-pointer"
			data-testid="persona-option"
			@select="$emit('toggle', persona)"
		>
			<Check
				:class="[
					'mr-2 h-4 w-4 shrink-0',
					selectedIds.includes(persona.id) ? 'opacity-100' : 'opacity-0',
				]"
			/>
			<PersonaAvatar
				:uuid="persona.id"
				:name="persona.name"
				:has-avatar="!!persona.avatar_url"
				:size="20"
				class="mr-2"
			/>
			<span class="min-w-0 flex-1 truncate">{{ persona.name }}</span>
			<span
				v-if="persona.workspace_id"
				class="ml-2 shrink-0 rounded bg-gray-100 px-1 text-2xs text-gray-500 dark:bg-gray-700 dark:text-gray-400"
			>
				workspace
			</span>
		</CommandItem>
	</CommandGroup>
</template>

<script lang="ts">
	import PersonaAvatar from '@/components/general/PersonaAvatar.vue';
	import { CommandGroup, CommandItem } from '@/components/ui/command';
	import type { PersonaAssignee } from '@/utils/personas';
	import { Check } from 'lucide-vue-next';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'AssigneePersonaGroup',
		components: { CommandGroup, CommandItem, PersonaAvatar, Check },
		props: {
			personas: {
				type: Array as PropType<PersonaAssignee[]>,
				default: () => [],
			},
			selectedIds: { type: Array as PropType<string[]>, default: () => [] },
		},
		emits: ['toggle'],
	});
</script>
