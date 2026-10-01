<template>
	<div v-if="personas.length" class="flex items-center" @click.stop>
		<Popover v-for="(persona, i) in personas" :key="persona.id">
			<PopoverTrigger as-child>
				<button
					type="button"
					class="flex shrink-0 rounded-full ring-2 ring-white dark:ring-gray-800"
					:class="{ '-ml-1.5': i > 0 }"
					:title="persona.name"
					:aria-label="`Persona ${persona.name}`"
					data-testid="persona-chip"
					@click.stop
				>
					<PersonaAvatar
						:uuid="persona.id"
						:name="persona.name"
						:has-avatar="!!persona.avatar_url"
						:size="size"
					/>
				</button>
			</PopoverTrigger>
			<PopoverContent
				class="z-50 w-60 border-gray-200 bg-white p-3 text-gray-900 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
				align="start"
				side="bottom"
				@click.stop
			>
				<div class="flex items-center gap-3" data-testid="persona-popover">
					<PersonaAvatar
						:uuid="persona.id"
						:name="persona.name"
						:has-avatar="!!persona.avatar_url"
						:size="40"
					/>
					<div class="min-w-0">
						<p class="truncate text-sm font-semibold">{{ persona.name }}</p>
						<p
							v-if="persona.description"
							class="line-clamp-2 text-xs text-gray-500 dark:text-gray-400"
						>
							{{ persona.description }}
						</p>
					</div>
				</div>
				<div
					class="mt-3 flex items-center gap-2 border-t border-gray-200 pt-3 text-xs text-gray-500 dark:border-gray-600 dark:text-gray-400"
				>
					<UserAvatar
						:user-id="persona.owner.id"
						:name="persona.owner.name"
						:has-avatar="ownerHasAvatar(persona)"
						:size="24"
					/>
					<span class="min-w-0 truncate">
						persona of
						<span class="font-medium text-gray-900 dark:text-gray-200">{{
							persona.owner.name
						}}</span>
					</span>
				</div>
			</PopoverContent>
		</Popover>
	</div>
</template>

<script lang="ts">
	import PersonaAvatar from '@/components/general/PersonaAvatar.vue';
	import UserAvatar from '@/components/general/UserAvatar.vue';
	import {
		Popover,
		PopoverContent,
		PopoverTrigger,
	} from '@/components/ui/popover';
	import type { PersonaAssignee } from '@/utils/personas';
	import { defineComponent, type PropType } from 'vue';

	export default defineComponent({
		name: 'PersonaAssigneeChips',
		components: {
			PersonaAvatar,
			UserAvatar,
			Popover,
			PopoverContent,
			PopoverTrigger,
		},
		props: {
			personas: {
				type: Array as PropType<PersonaAssignee[]>,
				default: () => [],
			},
			size: { type: Number, default: 22 },
			memberAvatars: {
				type: Object as PropType<Record<number, boolean>>,
				default: () => ({}),
			},
		},
		setup(props) {
			const ownerHasAvatar = (persona: PersonaAssignee) =>
				persona.owner.has_avatar ?? props.memberAvatars[persona.owner.id] ?? false;

			return { ownerHasAvatar };
		},
	});
</script>
