<template>
	<SettingsSection :title="$t('settings.slack.people.title')">
		<p v-if="!links.length" class="text-sm text-ink-subtle">
			{{ $t('settings.slack.people.empty') }}
		</p>
		<ul
			v-else
			class="divide-y divide-border flex flex-col rounded-md border border-border"
		>
			<li
				v-for="link in links"
				:key="link.user_id"
				class="flex items-center justify-between gap-3 px-3 py-2 text-sm text-ink"
			>
				<span class="min-w-0 truncate">
					{{ link.name }}
					<span class="text-ink-subtle">{{ link.slack_user_id }}</span>
				</span>
				<Button variant="outline" size="sm" @click="unlink(link.user_id)">
					{{ $t('settings.slack.unlink') }}
				</Button>
			</li>
		</ul>
	</SettingsSection>
</template>

<script lang="ts">
	import {
		listSlackLinks,
		unlinkSlackUser,
		type SlackLink,
	} from '@/actions/tmgr/slack';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { useToast } from '@/components/ui/toast';
	import { t } from '@/i18n';
	import { defineComponent, onMounted, ref, type PropType } from 'vue';

	export default defineComponent({
		name: 'SlackLinkedPeople',
		components: { SettingsSection, Button },
		props: {
			workspaceId: {
				type: [Number, String] as PropType<number | string>,
				required: true,
			},
		},
		emits: ['changed'],
		setup(props, { emit }) {
			const toaster = useToast();
			const links = ref<SlackLink[]>([]);

			const load = async () => {
				try {
					links.value = await listSlackLinks(props.workspaceId);
				} catch {
					toaster.toast({ title: t('settings.slack.people.loadFailed') });
				}
			};

			const unlink = async (userId: number) => {
				try {
					await unlinkSlackUser(props.workspaceId, userId);
					links.value = links.value.filter((link) => link.user_id !== userId);
					emit('changed');
				} catch {
					toaster.toast({ title: t('settings.slack.people.unlinkFailed') });
				}
			};

			onMounted(load);

			return { links, unlink };
		},
	});
</script>
