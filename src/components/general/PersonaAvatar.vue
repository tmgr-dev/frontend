<template>
	<span
		class="inline-flex shrink-0 items-center justify-center overflow-hidden rounded-full bg-violet-100 font-medium text-violet-700 dark:bg-violet-900/30 dark:text-violet-300"
		:style="{
			width: `${size}px`,
			height: `${size}px`,
			fontSize: `${Math.round(size / 2.4)}px`,
		}"
		:title="name || undefined"
	>
		<img
			v-if="url"
			:src="url"
			:alt="name || 'Persona avatar'"
			class="h-full w-full object-cover"
			@error="url = null"
		/>
		<template v-else>{{ initials }}</template>
	</span>
</template>

<script lang="ts">
	import { personaAvatarObjectUrl } from '@/actions/tmgr/personas';
	import { avatarInitials } from '@/utils/avatarInitials';
	import { computed, defineComponent, ref, watch } from 'vue';

	export default defineComponent({
		name: 'PersonaAvatar',
		props: {
			uuid: { type: String, required: true },
			name: { type: String, default: '' },
			hasAvatar: { type: Boolean, default: false },
			size: { type: Number, default: 32 },
		},
		setup(props) {
			const url = ref<string | null>(null);
			const initials = computed(() => avatarInitials(props.name));

			const load = async () => {
				url.value = null;
				if (!props.hasAvatar || !props.uuid) return;
				try {
					url.value = await personaAvatarObjectUrl(props.uuid);
				} catch {
				}
			};

			watch(() => [props.uuid, props.hasAvatar], load, { immediate: true });

			return { url, initials };
		},
	});
</script>
