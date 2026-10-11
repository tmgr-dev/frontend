<template>
	<SettingsSection
		v-if="promotions.length && sectionTitle"
		:title="sectionTitle"
		:description="sectionDescription"
	>
		<div class="flex flex-col gap-2">
			<PromotionCard
				v-for="promotion in promotions"
				:key="promotion.id"
				:promotion="promotion"
				:compact="compact"
				@click="click"
			/>
		</div>
	</SettingsSection>
	<div v-else-if="promotions.length" class="flex flex-col gap-2">
		<p v-if="heading" class="text-xs font-medium text-muted-foreground">
			{{ heading }}
		</p>
		<PromotionCard
			v-for="promotion in promotions"
			:key="promotion.id"
			:promotion="promotion"
			:compact="compact"
			@click="click"
		/>
	</div>
</template>

<script>
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import PromotionCard from '@/components/promotions/PromotionCard.vue';
	import { usePromotions } from '@/composable/usePromotions';
	import { defineComponent, watch } from 'vue';

	export default defineComponent({
		name: 'PromotionsList',
		components: { PromotionCard, SettingsSection },
		props: {
			placement: { type: String, required: true },
			compact: { type: Boolean, default: false },
			heading: { type: String, default: '' },
			sectionTitle: { type: String, default: '' },
			sectionDescription: { type: String, default: '' },
		},
		setup(props) {
			const { promotions, trackImpression, click } = usePromotions(
				props.placement,
			);
			watch(
				promotions,
				(list) => list.forEach((promotion) => trackImpression(promotion)),
				{ flush: 'post' },
			);
			return { promotions, trackImpression, click };
		},
	});
</script>
