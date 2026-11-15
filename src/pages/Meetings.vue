<template>
	<BaseLayout :title="$t('meetings.title')" :subtitle="intro">
		<template #body>
			<div class="flex flex-col gap-6">
				<p v-if="error" role="alert" class="text-sm text-destructive">
					{{ error }}
				</p>

				<section v-if="step === 'list'" class="flex flex-col gap-3">
					<div>
						<Button @click="startImport">{{
							$t('meetings.list.import')
						}}</Button>
					</div>
					<p v-if="!records.length" class="text-sm text-ink-subtle">
						{{ $t('meetings.list.empty') }}
					</p>
					<ul v-else class="flex flex-col gap-2">
						<li
							v-for="record in records"
							:key="record.id"
							class="flex flex-wrap items-center justify-between gap-2 rounded-card border border-line bg-surface p-3"
						>
							<div class="flex flex-col">
								<span class="text-sm font-medium text-ink">{{
									record.title
								}}</span>
								<span class="text-xs text-ink-subtle"
									>{{ record.date }} ·
									{{
										$t('meetings.list.tasksCreated', {
											count: Object.keys(record.created).length,
										})
									}}</span
								>
							</div>
							<div class="flex gap-2">
								<Button
									size="sm"
									variant="outline"
									@click="openRecord(record)"
									>{{ $t('meetings.list.open') }}</Button
								>
								<Button
									size="sm"
									variant="outline"
									@click="removeRecord(record)"
									>{{ $t('meetings.list.delete') }}</Button
								>
							</div>
						</li>
					</ul>
				</section>

				<section v-else-if="step === 'import'" class="flex flex-col gap-3">
					<h2 class="text-base font-semibold text-ink">
						{{ $t('meetings.import.title') }}
					</h2>
					<div class="grid gap-3 sm:grid-cols-2">
						<div class="flex flex-col gap-1">
							<Label for="meeting-title">{{
								$t('meetings.import.meetingTitle')
							}}</Label>
							<Input id="meeting-title" v-model="title" />
						</div>
						<div class="flex flex-col gap-1">
							<Label for="meeting-date">{{ $t('meetings.import.date') }}</Label>
							<Input id="meeting-date" v-model="date" type="date" />
						</div>
					</div>
					<div class="flex flex-col gap-1">
						<Label for="meeting-file">{{ $t('meetings.import.file') }}</Label>
						<input
							id="meeting-file"
							type="file"
							accept=".txt,.md,.srt,.vtt,text/plain"
							class="text-sm text-ink"
							@change="onFile"
						/>
						<p class="text-xs text-ink-subtle">
							{{ $t('meetings.import.fileHint') }}
						</p>
					</div>
					<div class="flex flex-col gap-1">
						<Label for="meeting-text">{{ $t('meetings.import.paste') }}</Label>
						<Textarea id="meeting-text" v-model="text" rows="10" />
					</div>
					<p v-if="parsed.segments.length" class="text-xs text-ink-subtle">
						{{
							$t('meetings.import.segments', { count: parsed.segments.length })
						}}
					</p>
					<p
						v-if="parsed.segments.length && !parsed.hasTimecodes"
						class="text-sm text-amber-600 dark:text-amber-400"
					>
						{{ $t('meetings.import.noTimecodes') }}
					</p>
					<label class="flex items-start gap-2 text-sm text-ink">
						<input v-model="informed" type="checkbox" class="mt-1" />
						<span>{{ $t('meetings.import.informed') }}</span>
					</label>

					<div
						v-if="lanOrigin"
						class="flex flex-col gap-2 rounded-md border border-amber-300 bg-amber-50 p-3 text-sm dark:border-amber-700 dark:bg-amber-950/40"
					>
						<p class="text-amber-800 dark:text-amber-200">
							{{ $t('meetings.import.lanNotice', { origin: lanOrigin }) }}
						</p>
						<div class="flex gap-2">
							<Button size="sm" @click="sendToLan">{{
								$t('meetings.import.lanSend', { origin: lanOrigin })
							}}</Button>
							<Button size="sm" variant="outline" @click="lanOrigin = null">{{
								$t('meetings.import.lanCancel')
							}}</Button>
						</div>
					</div>

					<p v-if="aiOrigin" class="text-xs text-ink-subtle">
						{{ $t('meetings.import.aiNote', { origin: aiOrigin }) }}
					</p>
					<p v-else class="text-xs text-ink-subtle">
						{{ $t('meetings.import.noAi') }}
						<RouterLink
							:to="{ name: 'PersonaSettings' }"
							class="text-primary hover:underline"
							>{{ $t('meetings.import.setUpAi') }}</RouterLink
						>
					</p>

					<div class="flex flex-wrap gap-2">
						<Button
							v-if="aiOrigin && !finding"
							:disabled="!canContinue"
							@click="find"
							>{{ $t('meetings.import.find') }}</Button
						>
						<Button v-if="finding" variant="outline" @click="cancelFind">{{
							$t('meetings.import.cancel')
						}}</Button>
						<Button
							variant="outline"
							:disabled="!canContinue || finding"
							@click="manual"
							>{{ $t('meetings.import.manual') }}</Button
						>
						<Button variant="ghost" :disabled="finding" @click="toList">{{
							$t('meetings.import.back')
						}}</Button>
					</div>
					<p v-if="finding" class="text-sm text-ink-subtle">
						{{ $t('meetings.import.finding', progress) }}
					</p>
				</section>

				<section
					v-else-if="step === 'review' && current"
					class="flex flex-col gap-4"
				>
					<h2 class="text-base font-semibold text-ink">
						{{ $t('meetings.review.title') }}
					</h2>
					<p class="text-sm text-ink-subtle">
						{{
							current.items.length
								? $t('meetings.review.found', { count: current.items.length })
								: $t('meetings.review.foundNone')
						}}
					</p>
					<p v-if="dropped" class="text-xs text-ink-subtle">
						{{ $t('meetings.review.dropped', { count: dropped }) }}
					</p>

					<ul class="flex flex-col gap-3">
						<li
							v-for="item in current.items"
							:key="item.key"
							class="flex flex-col gap-2 rounded-card border border-line bg-surface p-3"
						>
							<div class="flex items-start gap-2">
								<input
									v-model="item.selected"
									type="checkbox"
									class="mt-2"
									:disabled="!!current.created[item.key]?.commented"
									:aria-label="item.title"
								/>
								<div class="flex flex-1 flex-col gap-1">
									<Label>{{ $t('meetings.review.titleLabel') }}</Label>
									<Input v-model="item.title" />
								</div>
								<span
									v-if="item.startMs !== null"
									class="mt-2 text-xs text-ink-subtle"
									>{{ timecode(item.startMs) }}</span
								>
							</div>
							<p v-if="item.why" class="text-xs text-ink-subtle">
								<strong>{{ $t('meetings.review.why') }}:</strong> {{ item.why }}
							</p>
							<p class="text-xs text-ink-subtle">
								<strong>{{ $t('meetings.review.quote') }}:</strong> “{{
									item.quote
								}}”
							</p>
						</li>
					</ul>

					<details class="rounded-card border border-line bg-surface p-3">
						<summary class="cursor-pointer text-sm font-medium text-ink">
							{{ $t('meetings.review.addFromTranscript') }}
						</summary>
						<ul class="mt-2 flex max-h-72 flex-col gap-1 overflow-y-auto">
							<li
								v-for="(segment, index) in segments"
								:key="index"
								class="flex items-start justify-between gap-2 text-sm"
							>
								<span class="text-ink"
									><span
										v-if="segment.startMs !== null"
										class="text-xs text-ink-subtle"
										>{{ timecode(segment.startMs) }}
									</span>
									<span v-if="segment.speaker" class="font-medium"
										>{{ segment.speaker }}: </span
									>{{ segment.text }}</span
								>
								<Button
									size="sm"
									variant="outline"
									@click="addSegment(segment)"
									>{{ $t('meetings.review.add') }}</Button
								>
							</li>
						</ul>
					</details>

					<label class="flex items-start gap-2 text-sm text-ink">
						<input v-model="withPage" type="checkbox" class="mt-1" />
						<span>{{ $t('meetings.review.asPage') }}</span>
					</label>
					<p
						v-if="workspaceName"
						class="text-xs"
						:class="
							isCloud ? 'text-amber-600 dark:text-amber-400' : 'text-ink-subtle'
						"
					>
						{{
							isCloud
								? $t('meetings.review.cloudNotice', {
										workspace: workspaceName,
									})
								: $t('meetings.review.localNotice', {
										workspace: workspaceName,
									})
						}}
					</p>
					<p v-else class="text-sm text-destructive">
						{{ $t('meetings.review.noWorkspace') }}
					</p>

					<div class="flex flex-wrap gap-2">
						<Button
							:disabled="creating || !selectedCount || !workspaceName"
							@click="create"
							>{{
								creating
									? $t('meetings.review.creating')
									: $t('meetings.review.create', { count: selectedCount })
							}}</Button
						>
						<Button variant="ghost" :disabled="creating" @click="toList">{{
							$t('meetings.import.back')
						}}</Button>
					</div>

					<div
						v-if="result"
						class="flex flex-col gap-2 rounded-card border border-line bg-surface p-3 text-sm text-ink"
					>
						<p>
							{{
								$t('meetings.done.summary', {
									tasks: result.tasksCreated,
									comments: result.commentsCreated,
								})
							}}
						</p>
						<p v-if="result.pageCreated">{{ $t('meetings.done.page') }}</p>
						<template v-if="result.errors.length">
							<p class="text-destructive">{{ $t('meetings.done.failed') }}</p>
							<ul class="list-disc pl-5 text-xs text-destructive">
								<li v-for="message in result.errors" :key="message">
									{{ message }}
								</li>
							</ul>
							<div>
								<Button
									size="sm"
									variant="outline"
									:disabled="creating"
									@click="create"
									>{{ $t('meetings.done.retry') }}</Button
								>
							</div>
						</template>
					</div>
				</section>
			</div>
		</template>
	</BaseLayout>
</template>

<script lang="ts">
	import { t } from '@/i18n';
	import { createComment } from '@/actions/tmgr/comments';
	import { createPage } from '@/actions/tmgr/pages';
	import { getStatusesOfWorkspace } from '@/actions/tmgr/statuses';
	import { createTask } from '@/actions/tmgr/tasks';
	import BaseLayout from '@/components/layouts/BaseLayout.vue';
	import { Button } from '@/components/ui/button';
	import { Input } from '@/components/ui/input';
	import { Label } from '@/components/ui/label';
	import { Textarea } from '@/components/ui/textarea';
	import {
		llmComplete,
		llmLanConfirm,
		needsConfirmationOrigin,
	} from '@/local/llmClient';
	import { LOCAL_OWNER_ID } from '@/local/personaGate';
	import { resyncCloudIdentity } from '@/utils/cloudIdentity';
	import { pickDefaultStatusId } from '@/utils/defaultStatus';
	import {
		extractItems,
		itemKey,
		manualItem,
		type DraftItem,
	} from '@/utils/meetings/extraction';
	import {
		createOutputs,
		type CreateResult,
		type MeetingRecord,
		type StoredItem,
	} from '@/utils/meetings/meetingCreate';
	import {
		RETENTION_DAYS,
		deleteMeeting,
		loadMeetings,
		newMeetingId,
		saveMeeting,
	} from '@/utils/meetings/meetingStore';
	import {
		formatTimecode,
		parseTranscript,
		type TranscriptSegment,
	} from '@/utils/meetings/transcriptParser';
	import { computed, defineComponent, onMounted, ref } from 'vue';
	import { RouterLink } from 'vue-router';
	import { useStore } from 'vuex';

	const MAX_CHARS = 1_000_000;

	const messageOf = (error: unknown): string =>
		error instanceof Error ? error.message : String(error);

	const originOf = (url: string): string | null => {
		try {
			const { origin } = new URL(url);
			return origin === 'null' ? null : origin;
		} catch {
			return null;
		}
	};

	const today = () => new Date().toLocaleDateString('en-CA');

	export default defineComponent({
		name: 'MeetingsPage',
		components: { BaseLayout, Button, Input, Label, RouterLink, Textarea },
		setup() {
			const store = useStore();
			const localOnly = !!store.getters?.isLocalOnly;
			const userId = () =>
				localOnly ? LOCAL_OWNER_ID : Number(store.state.user?.id) || 0;

			const step = ref<'list' | 'import' | 'review'>('list');
			const records = ref<MeetingRecord[]>([]);
			const error = ref<string | null>(null);
			const title = ref('');
			const date = ref(today());
			const text = ref('');
			const informed = ref(false);
			const aiOrigin = ref<string | null>(null);
			const lanOrigin = ref<string | null>(null);
			const finding = ref(false);
			const cancelled = ref(false);
			const progress = ref({ done: 0, total: 0 });
			const current = ref<MeetingRecord | null>(null);
			const dropped = ref(0);
			const withPage = ref(false);
			const creating = ref(false);
			const result = ref<CreateResult | null>(null);

			const intro = computed(() =>
				t('meetings.intro', { days: RETENTION_DAYS }),
			);
			const parsed = computed(() => parseTranscript(text.value));
			const segments = computed(() =>
				current.value ? parseTranscript(current.value.transcript).segments : [],
			);
			const canContinue = computed(
				() => parsed.value.segments.length > 0 && informed.value,
			);
			const selectedCount = computed(
				() => current.value?.items.filter((item) => item.selected).length ?? 0,
			);
			const workspace = computed(() => store.getters.currentWorkspace);
			const workspaceName = computed(() => workspace.value?.name ?? '');
			const isCloud = computed(
				() => !localOnly && Number(workspace.value?.id) >= 0,
			);

			const timecode = (ms: number) => formatTimecode(ms);

			const reload = () => {
				try {
					records.value = loadMeetings(localStorage, Date.now()).sort(
						(a, b) => b.importedAt - a.importedAt,
					);
				} catch (caught) {
					error.value = messageOf(caught);
				}
			};

			const baseUrl = async (): Promise<string> => {
				await resyncCloudIdentity();
				const core = await import('@tauri-apps/api/core');
				const config = await core.invoke<{ base_url: string }>(
					'llm_config_get',
					{ userId: userId() },
				);
				return config.base_url;
			};

			onMounted(async () => {
				reload();
				aiOrigin.value = await baseUrl().then(
					(url) => (url ? originOf(url) : null),
					() => null,
				);
			});

			const toList = () => {
				error.value = null;
				result.value = null;
				lanOrigin.value = null;
				step.value = 'list';
				reload();
			};

			const startImport = () => {
				error.value = null;
				title.value = '';
				date.value = today();
				text.value = '';
				informed.value = false;
				step.value = 'import';
			};

			const onFile = async (event: Event) => {
				const file = (event.target as HTMLInputElement).files?.[0];
				if (!file) return;
				if (file.size > MAX_CHARS) {
					error.value = t('meetings.import.tooLarge', { limit: '1 MB' });
					return;
				}
				error.value = null;
				text.value = await file.text();
				if (!title.value) title.value = file.name.replace(/\.[^.]+$/, '');
			};

			const buildRecord = (items: DraftItem[]): MeetingRecord => {
				const id = newMeetingId();
				return {
					id,
					title: title.value.trim() || t('meetings.import.meetingTitleDefault'),
					date: date.value || today(),
					importedAt: Date.now(),
					transcript: text.value,
					participantsInformed: informed.value,
					items: items.map((item): StoredItem => ({
						...item,
						key: itemKey(id, item),
						selected: true,
					})),
					created: {},
					pageId: null,
				};
			};

			const enterReview = (record: MeetingRecord, droppedCount: number) => {
				try {
					saveMeeting(localStorage, record);
				} catch (caught) {
					error.value = messageOf(caught);
					return;
				}
				current.value = record;
				dropped.value = droppedCount;
				result.value = null;
				withPage.value = false;
				step.value = 'review';
			};

			const guard = (): boolean => {
				error.value = null;
				if (text.value.length > MAX_CHARS) {
					error.value = t('meetings.import.tooLarge', { limit: '1 MB' });
					return false;
				}
				if (!parsed.value.segments.length) {
					error.value = t('meetings.import.empty');
					return false;
				}
				if (!informed.value) {
					error.value = t('meetings.import.informedRequired');
					return false;
				}
				return true;
			};

			const manual = () => {
				if (!guard()) return;
				enterReview(buildRecord([]), 0);
			};

			const find = async () => {
				if (!guard()) return;
				finding.value = true;
				cancelled.value = false;
				lanOrigin.value = null;
				progress.value = { done: 0, total: 0 };
				try {
					const outcome = await extractItems(
						parsed.value,
						async (system, prompt) =>
							llmComplete(userId(), {
								system,
								prompt,
								maxTokens: 2000,
								temperature: 0.2,
							}),
						{
							isCancelled: () => cancelled.value,
							onProgress: (done, total) => {
								progress.value = { done, total };
							},
						},
					);
					if (outcome.cancelled) return;
					enterReview(buildRecord(outcome.items), outcome.dropped);
				} catch (caught) {
					const origin = needsConfirmationOrigin(caught);
					if (origin) {
						lanOrigin.value = origin;
						return;
					}
					error.value = messageOf(caught);
				} finally {
					finding.value = false;
				}
			};

			const sendToLan = async () => {
				const origin = lanOrigin.value;
				if (!origin) return;
				lanOrigin.value = null;
				try {
					await llmLanConfirm(userId(), origin, true);
				} catch (caught) {
					error.value = messageOf(caught);
					return;
				}
				await find();
			};

			const cancelFind = () => {
				cancelled.value = true;
			};

			const openRecord = (record: MeetingRecord) => {
				error.value = null;
				current.value = structuredClone(record);
				dropped.value = 0;
				result.value = null;
				withPage.value = false;
				step.value = 'review';
			};

			const removeRecord = (record: MeetingRecord) => {
				if (!window.confirm(t('meetings.list.deleteConfirm'))) return;
				try {
					deleteMeeting(localStorage, record.id);
				} catch (caught) {
					error.value = messageOf(caught);
				}
				reload();
			};

			const addSegment = (segment: TranscriptSegment) => {
				const record = current.value;
				if (!record) return;
				const item = manualItem(segment);
				const key = itemKey(record.id, item);
				if (record.items.some((existing) => existing.key === key)) return;
				record.items.push({ ...item, key, selected: true });
			};

			const create = async () => {
				const record = current.value;
				const ws = workspace.value;
				if (!record || !ws) return;
				error.value = null;
				creating.value = true;
				try {
					const statusId = pickDefaultStatusId(
						await getStatusesOfWorkspace(ws.id),
					);
					if (statusId === undefined) {
						error.value = t('meetings.review.noWorkspace');
						return;
					}
					const outcome = await createOutputs(
						{
							createTask: (payload) => createTask(payload as never),
							createComment: (taskId, payload) =>
								createComment(taskId, payload),
							createPage: (payload) => createPage(payload as never),
						},
						record,
						{ workspaceId: ws.id, statusId },
						{
							withPage: withPage.value,
							save: (next) => saveMeeting(localStorage, next),
						},
					);
					current.value = outcome.record;
					result.value = outcome;
				} catch (caught) {
					error.value = messageOf(caught);
				} finally {
					creating.value = false;
				}
			};

			return {
				step,
				records,
				error,
				title,
				date,
				text,
				informed,
				aiOrigin,
				lanOrigin,
				finding,
				progress,
				current,
				dropped,
				withPage,
				creating,
				result,
				intro,
				parsed,
				segments,
				canContinue,
				selectedCount,
				workspaceName,
				isCloud,
				timecode,
				toList,
				startImport,
				onFile,
				manual,
				find,
				sendToLan,
				cancelFind,
				openRecord,
				removeRecord,
				addSegment,
				create,
			};
		},
	});
</script>
