<template>
	<span
		class="bg-brand/10 inline-flex shrink-0 items-center justify-center overflow-hidden rounded-full font-medium text-brand"
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
			:alt="name || 'Avatar'"
			class="h-full w-full object-cover"
			@error="url = null"
		/>
		<template v-else>{{ initials }}</template>
	</span>
</template>

<script lang="ts">
	import { avatarUrl } from '@/actions/tmgr/avatars';
	import { avatarInitials } from '@/utils/avatarInitials';
	import { computed, defineComponent, ref, watch } from 'vue';

	/**
	 * TM-142 — a user's picture, with their initials underneath it. The picture is loaded through a
	 * signed link, so `hasAvatar` (which every payload naming a user carries) decides whether to ask
	 * for one at all; a user without a picture costs no request.
	 */
	export default defineComponent({
		name: 'UserAvatar',
		props: {
			userId: { type: Number, required: true },
			name: { type: String, default: '' },
			hasAvatar: { type: Boolean, default: false },
			size: { type: Number, default: 32 },
		},
		setup(props) {
			const url = ref<string | null>(null);
			const initials = computed(() => avatarInitials(props.name));

			const load = async () => {
				url.value = null;

				if (!props.hasAvatar || !props.userId) {
					return;
				}

				try {
					url.value = await avatarUrl(props.userId);
				} catch {
					// The initials are a good enough answer to a link that cannot be minted.
				}
			};

			watch(() => [props.userId, props.hasAvatar], load, { immediate: true });

			return { url, initials };
		},
	});
</script>
