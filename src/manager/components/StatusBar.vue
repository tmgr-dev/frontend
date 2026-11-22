<template>
	<footer class="mgr-statusbar">
		<span class="crumb"
			>{{ scope }}<i aria-hidden="true">&rsaquo;</i><b>{{ chat }}</b></span
		>
		<span class="sp"></span>
		<span v-if="dnd" class="item">{{ $t('manager.statusbar.dnd') }}</span>
		<span v-if="live" class="item live"
			><i class="dot"></i>{{ $t('manager.statusbar.live') }}</span
		>
		<span v-if="version" class="item">v{{ version }}</span>
	</footer>
</template>

<script lang="ts">
	import { defineComponent } from 'vue';

	export default defineComponent({
		name: 'ManagerStatusBar',
		props: {
			scope: { type: String, required: true },
			chat: { type: String, default: '' },
			live: { type: Boolean, default: false },
			dnd: { type: Boolean, default: false },
		},
		data() {
			return { version: '' };
		},
		async mounted() {
			try {
				const { getVersion } = await import('@tauri-apps/api/app');
				this.version = await getVersion();
			} catch {
				this.version = '';
			}
		},
	});
</script>
