<template>
	<span class="inline-flex min-w-0 items-center gap-1.5" :title="tooltip">
		<span v-if="!hideAvatar" class="relative inline-flex shrink-0">
			<PersonaAvatar
				v-if="resolved.kind === 'persona'"
				:uuid="resolved.id"
				:name="resolved.name"
				:has-avatar="!!resolved.avatar"
				:size="size"
			/>
			<UserAvatar
				v-else-if="resolved.kind === 'user'"
				:user-id="Number(resolved.id) || 0"
				:name="resolved.name"
				:has-avatar="!!user?.has_avatar"
				:size="size"
			/>
			<span
				v-else
				class="inline-flex shrink-0 items-center justify-center overflow-hidden rounded-full bg-gray-100 font-medium text-gray-600 dark:bg-gray-700 dark:text-gray-300"
				:style="{ width: `${size}px`, height: `${size}px` }"
			>
				<Plug v-if="resolved.kind === 'plugin'" :style="iconStyle" />
				<Cog v-else-if="resolved.kind === 'system'" :style="iconStyle" />
				<template v-else>{{ initials }}</template>
			</span>
			<span
				v-if="resolved.kind === 'persona'"
				class="absolute -bottom-0.5 -right-0.5 flex h-3.5 w-3.5 items-center justify-center rounded-full bg-white ring-1 ring-white dark:bg-gray-900 dark:ring-gray-900"
			>
				<VenetianMask class="h-2.5 w-2.5 text-violet-600 dark:text-violet-400" />
			</span>
		</span>
		<span v-if="showLabel" class="inline-flex min-w-0 flex-col leading-tight">
			<span class="truncate text-sm font-semibold text-ink">{{ label }}</span>
			<span
				v-if="resolved.subtitle"
				class="truncate text-2xs text-ink-subtle"
			>
				{{ resolved.subtitle }}
			</span>
		</span>
	</span>
</template>

<script lang="ts">
	import { resolveAuthor } from '@/utils/personas';
	import type { AuthorRef } from '@/types/author';
	import { avatarInitials } from '@/utils/avatarInitials';
	import { Cog, Plug, VenetianMask } from 'lucide-vue-next';
	import { computed, defineComponent, type PropType } from 'vue';
	import PersonaAvatar from './PersonaAvatar.vue';
	import UserAvatar from './UserAvatar.vue';

	interface FallbackUser {
		id: number | string;
		name: string;
		has_avatar?: boolean;
	}

	export default defineComponent({
		name: 'AuthorBadge',
		components: { PersonaAvatar, UserAvatar, Cog, Plug, VenetianMask },
		props: {
			author: { type: Object as PropType<AuthorRef | null | undefined>, default: null },
			user: { type: Object as PropType<FallbackUser | null>, default: null },
			size: { type: Number, default: 20 },
			showLabel: { type: Boolean, default: true },
			hideAvatar: { type: Boolean, default: false },
			permissions: { type: Array as PropType<string[] | null>, default: null },
		},
		setup(props) {
			const resolved = computed(() => resolveAuthor(props.author, props.user));
			const initials = computed(() => avatarInitials(resolved.value.name));
			const iconStyle = computed(() => ({
				width: `${Math.round(props.size / 1.8)}px`,
				height: `${Math.round(props.size / 1.8)}px`,
			}));
			const label = computed(() => {
				const known = ['user', 'persona', 'plugin', 'system'].includes(resolved.value.kind);
				return known
					? resolved.value.name
					: `${resolved.value.kind}: ${resolved.value.name}`;
			});
			const tooltip = computed(() => {
				const parts = [resolved.value.subtitle].filter(Boolean) as string[];
				if (props.permissions?.length) {
					parts.push(`Permissions: ${props.permissions.join(', ')}`);
				}
				return parts.join(' — ') || undefined;
			});

			return { resolved, initials, iconStyle, label, tooltip };
		},
	});
</script>
