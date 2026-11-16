<template>
	<SettingsSection
		:title="$t('settings.slack.channels.title')"
		:description="$t('settings.slack.channels.inviteHint')"
	>
		<form class="mb-4 flex gap-2" @submit.prevent="add">
			<Input
				:model-value="newChannelId"
				:placeholder="$t('settings.slack.channels.idPlaceholder')"
				@update:model-value="(v) => (newChannelId = String(v))"
			/>
			<Button type="submit" :disabled="!newChannelId.trim() || adding">
				{{ $t('settings.slack.channels.add') }}
			</Button>
		</form>

		<p v-if="!channels.length" class="text-sm text-ink-subtle">
			{{ $t('settings.slack.channels.empty') }}
		</p>
		<ul v-else class="flex flex-col gap-3">
			<li
				v-for="channel in channels"
				:key="channel.channel_id"
				class="rounded-md border border-border p-3"
			>
				<div class="flex items-center justify-between gap-3 text-sm text-ink">
					<span class="min-w-0 truncate">
						#{{ channel.name }}
						<span class="text-ink-subtle">{{ channel.channel_id }}</span>
						<span
							v-if="channel.shared"
							class="ml-1 rounded bg-muted px-1.5 py-0.5 text-xs"
						>
							{{ $t('settings.slack.channels.shared') }}
						</span>
					</span>
					<span class="flex shrink-0 gap-2">
						<Button variant="outline" size="sm" @click="toggle(channel)">
							{{ $t('settings.slack.subscriptions.title') }}
						</Button>
						<Button variant="outline" size="sm" @click="remove(channel)">
							{{ $t('settings.slack.channels.remove') }}
						</Button>
					</span>
				</div>

				<div
					v-if="openId === channel.channel_id"
					class="mt-3 flex flex-col gap-3"
				>
					<ul
						v-if="subscriptions.length"
						class="divide-y divide-border flex flex-col rounded-md border border-border"
					>
						<li
							v-for="sub in subscriptions"
							:key="sub.id"
							class="flex items-center justify-between gap-3 px-3 py-2 text-sm text-ink"
						>
							<span class="min-w-0">
								{{ categoryTitle(sub.project_category_id) }}:
								{{ sub.events.map(eventLabel).join(', ') }}
								<span class="text-ink-subtle">
									{{
										sub.min_priority ??
										$t('settings.slack.subscriptions.anyPriority')
									}}
									/ {{ $t(`settings.slack.subscriptions.mode_${sub.mode}`) }}
								</span>
							</span>
							<Button
								variant="outline"
								size="sm"
								@click="removeSubscription(channel, sub.id)"
							>
								{{ $t('settings.slack.subscriptions.delete') }}
							</Button>
						</li>
					</ul>

					<form
						class="flex flex-col gap-3"
						@submit.prevent="subscribe(channel)"
					>
						<select v-model="draft.project_category_id" :class="SELECT">
							<option :value="null">
								{{ $t('settings.slack.subscriptions.wholeWorkspace') }}
							</option>
							<option
								v-for="category in categories"
								:key="category.id"
								:value="category.id"
							>
								{{ category.title }}
							</option>
						</select>
						<div class="flex flex-wrap gap-x-4 gap-y-1">
							<label
								v-for="event in EVENTS"
								:key="event"
								class="flex items-center gap-2 text-sm text-ink"
							>
								<input
									type="checkbox"
									:value="event"
									v-model="draft.events"
									class="h-4 w-4 rounded border-border accent-primary"
								/>
								{{ eventLabel(event) }}
							</label>
						</div>
						<div class="flex flex-wrap gap-2">
							<select v-model="draft.min_priority" :class="SELECT">
								<option :value="null">
									{{ $t('settings.slack.subscriptions.anyPriority') }}
								</option>
								<option v-for="p in PRIORITIES" :key="p" :value="p">
									{{ p }}
								</option>
							</select>
							<select v-model="draft.mode" :class="SELECT">
								<option v-for="m in MODES" :key="m" :value="m">
									{{ $t(`settings.slack.subscriptions.mode_${m}`) }}
								</option>
							</select>
						</div>
						<div>
							<Button type="submit" :disabled="!draft.events.length">
								{{ $t('settings.slack.subscriptions.add') }}
							</Button>
						</div>
					</form>
				</div>
			</li>
		</ul>
	</SettingsSection>
</template>

<script lang="ts">
	import {
		getCategoriesOfWorkspace,
		type Category,
	} from '@/actions/tmgr/categories';
	import {
		addSlackChannel,
		createSlackSubscription,
		deleteSlackSubscription,
		listSlackChannels,
		listSlackSubscriptions,
		removeSlackChannel,
		type SlackChannel,
		type SlackEvent,
		type SlackSubscription,
		type SlackSubscriptionInput,
	} from '@/actions/tmgr/slack';
	import SettingsSection from '@/components/layouts/SettingsSection.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { useToast } from '@/components/ui/toast';
	import { t } from '@/i18n';
	import { defineComponent, onMounted, reactive, ref } from 'vue';

	const SELECT =
		'h-9 rounded-md border border-border bg-background px-2 text-sm text-ink';
	const EVENTS: SlackEvent[] = [
		'task_created',
		'task_status_changed',
		'task_assigned',
		'comment_created',
	];
	const PRIORITIES = ['low', 'medium', 'high', 'urgent'] as const;
	const MODES = ['thread', 'flat'] as const;

	export default defineComponent({
		name: 'SlackChannelsPanel',
		components: { SettingsSection, Button, Input },
		props: { workspaceId: { type: Number, required: true } },
		setup(props) {
			const toaster = useToast();
			const channels = ref<SlackChannel[]>([]);
			const categories = ref<Category[]>([]);
			const subscriptions = ref<SlackSubscription[]>([]);
			const openId = ref<string | null>(null);
			const newChannelId = ref('');
			const adding = ref(false);

			const draft = reactive<SlackSubscriptionInput>({
				project_category_id: null,
				events: [],
				min_priority: null,
				mode: 'thread',
			});

			const eventLabel = (event: string) =>
				t(`settings.slack.subscriptions.events.${event}`);

			const categoryTitle = (id: number | null) =>
				id === null
					? t('settings.slack.subscriptions.wholeWorkspace')
					: (categories.value.find((c) => c.id === id)?.title ?? `#${id}`);

			const fail = (key: string) => toaster.toast({ title: t(key) });

			const load = async () => {
				try {
					channels.value = await listSlackChannels(props.workspaceId);
				} catch {
					fail('settings.slack.channels.loadFailed');
				}
			};

			const add = async () => {
				adding.value = true;
				try {
					const channel = await addSlackChannel(
						props.workspaceId,
						newChannelId.value.trim(),
					);
					channels.value = [
						...channels.value.filter(
							(c) => c.channel_id !== channel.channel_id,
						),
						channel,
					];
					newChannelId.value = '';
				} catch {
					fail('settings.slack.channels.addFailed');
				} finally {
					adding.value = false;
				}
			};

			const remove = async (channel: SlackChannel) => {
				try {
					await removeSlackChannel(props.workspaceId, channel.channel_id);
					channels.value = channels.value.filter(
						(c) => c.channel_id !== channel.channel_id,
					);
					if (openId.value === channel.channel_id) openId.value = null;
				} catch {
					fail('settings.slack.channels.removeFailed');
				}
			};

			const toggle = async (channel: SlackChannel) => {
				if (openId.value === channel.channel_id) {
					openId.value = null;
					return;
				}
				openId.value = channel.channel_id;
				subscriptions.value = [];
				try {
					subscriptions.value = await listSlackSubscriptions(
						props.workspaceId,
						channel.channel_id,
					);
				} catch {
					fail('settings.slack.subscriptions.loadFailed');
				}
			};

			const subscribe = async (channel: SlackChannel) => {
				try {
					const created = await createSlackSubscription(
						props.workspaceId,
						channel.channel_id,
						{ ...draft, events: [...draft.events] },
					);
					subscriptions.value = [...subscriptions.value, created];
					draft.events = [];
				} catch {
					fail('settings.slack.subscriptions.addFailed');
				}
			};

			const removeSubscription = async (channel: SlackChannel, id: number) => {
				try {
					await deleteSlackSubscription(
						props.workspaceId,
						channel.channel_id,
						id,
					);
					subscriptions.value = subscriptions.value.filter((s) => s.id !== id);
				} catch {
					fail('settings.slack.subscriptions.deleteFailed');
				}
			};

			onMounted(async () => {
				load();
				try {
					categories.value = await getCategoriesOfWorkspace(props.workspaceId);
				} catch {
					categories.value = [];
				}
			});

			return {
				SELECT,
				EVENTS,
				PRIORITIES,
				MODES,
				channels,
				categories,
				subscriptions,
				openId,
				newChannelId,
				adding,
				draft,
				eventLabel,
				categoryTitle,
				add,
				remove,
				toggle,
				subscribe,
				removeSubscription,
			};
		},
	});
</script>
