<template>
	<PageContainer width="narrow">
		<PageHeader title="Settings" />
		<SettingsNav variant="list" />
	</PageContainer>
</template>

<script lang="ts">
	import PageContainer from '@/components/layouts/PageContainer.vue';
	import PageHeader from '@/components/layouts/PageHeader.vue';
	import { defineComponent } from 'vue';
	import SettingsNav from './SettingsNav.vue';

	export default defineComponent({
		name: 'SettingsSections',
		components: { PageContainer, PageHeader, SettingsNav },
	});
</script>
