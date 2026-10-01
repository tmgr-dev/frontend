<template>
	<div data-testid="page-properties">
		<div class="mb-4 flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
			<span
				class="inline-flex items-center rounded-full bg-gray-100 px-2.5 py-0.5 text-xs font-medium text-gray-700 dark:bg-gray-700 dark:text-gray-200"
				data-testid="page-type-badge"
				>{{ typeLabel }}</span
			>
			<span
				v-if="page.author"
				class="inline-flex items-center gap-1.5 text-ink-subtle"
				data-testid="page-author"
			>
				<span class="text-xs">Author</span>
				<AuthorBadge :author="author" :size="18" />
			</span>
			<span
				v-if="page.updated_by"
				class="inline-flex items-center gap-1.5 text-ink-subtle"
				data-testid="page-updated-by"
			>
				<span class="text-xs">Edited by</span>
				<AuthorBadge :author="updatedBy" :size="18" />
				<span class="text-xs">{{ updatedAt }}</span>
			</span>
		</div>

		<dl
			v-if="defs.length"
			class="mb-4 grid grid-cols-[minmax(7rem,max-content)_minmax(0,1fr)] items-start gap-x-4 gap-y-2 text-sm"
			data-testid="page-property-fields"
		>
			<template v-for="def in defs" :key="def.key">
				<dt class="pt-0.5 text-xs text-ink-subtle">{{ def.label }}</dt>
				<dd class="min-w-0" :data-property="def.key">
					<select
						v-if="def.kind === 'enum'"
						:value="valueOf(def.key) ?? ''"
						class="rounded border border-gray-300 bg-white px-2 py-0.5 text-sm text-gray-900 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
						@change="
							setText(def.key, ($event.target as HTMLSelectElement).value)
						"
					>
						<option value=""></option>
						<option v-for="o in def.options" :key="o.value" :value="o.value">
							{{ o.label }}
						</option>
					</select>
					<span
						v-else-if="def.kind === 'readonlyDate'"
						class="text-gray-700 dark:text-gray-300"
						data-testid="last-contact"
						>{{ valueOf(def.key) || '—' }}</span
					>
					<AliasesEditor
						v-else-if="def.kind === 'aliases'"
						:model-value="valueOf(def.key) ?? []"
						:errors="errors"
						@update:model-value="set(def.key, $event)"
					/>
					<ChipPicker
						v-else-if="def.kind === 'user'"
						single
						add-label="Select"
						placeholder="Member name"
						:chips="userChips"
						:search="searchMembers"
						@add="set(def.key, Number($event))"
						@remove="set(def.key, null)"
						@open="open('user', $event)"
					/>
					<ChipPicker
						v-else-if="def.kind === 'participants'"
						add-label="Member"
						placeholder="Person or member"
						:chips="participantChips"
						:search="searchParticipants"
						@add="set(def.key, [...(valueOf(def.key) ?? []), $event])"
						@remove="
							set(
								def.key,
								(valueOf(def.key) ?? []).filter((v: string) => v !== $event),
							)
						"
						@open="openParticipant($event)"
					/>
					<ChipPicker
						v-else-if="def.kind === 'tasks'"
						add-label="Task"
						placeholder="Search tasks"
						:chips="taskChips"
						:search="searchTasks"
						@add="set(def.key, [...(valueOf(def.key) ?? []), Number($event)])"
						@remove="
							set(
								def.key,
								(valueOf(def.key) ?? []).filter(
									(v: number) => String(v) !== $event,
								),
							)
						"
						@open="open('task', $event)"
					/>
					<input
						v-else
						:type="def.kind === 'date' ? 'date' : 'text'"
						:value="valueOf(def.key) ?? ''"
						class="rounded border bg-white px-2 py-0.5 text-sm text-gray-900 dark:bg-gray-800 dark:text-gray-100"
						:class="
							errorFor(errors, def.key)
								? 'border-red-500'
								: 'border-gray-300 dark:border-gray-600'
						"
						@change="
							setText(def.key, ($event.target as HTMLInputElement).value)
						"
					/>
					<p
						v-if="errorFor(errors, def.key)"
						class="mt-0.5 text-xs text-red-600 dark:text-red-400"
						role="alert"
						data-testid="property-error"
					>
						{{ errorFor(errors, def.key) }}
					</p>
				</dd>
			</template>
		</dl>
	</div>
</template>

<script lang="ts">
	import type { Page } from '@/actions/tmgr/pages';
	import AuthorBadge from '@/components/general/AuthorBadge.vue';
	import { toAuthorRef } from '@/utils/pages/author';
	import {
		blankToNull,
		errorFor,
		parseParticipant,
		type PropertyErrors,
		withProperty,
	} from '@/utils/pages/properties';
	import { computed, defineComponent, type PropType, watch } from 'vue';
	import AliasesEditor from './AliasesEditor.vue';
	import ChipPicker, { type ChipOption } from './ChipPicker.vue';
	import { propertyDefsFor, TYPE_LABELS } from './propertyDefs';
	import type { TmgrDirectory } from './useTmgrDirectory';

	export default defineComponent({
		name: 'PageProperties',
		components: { AliasesEditor, AuthorBadge, ChipPicker },
		props: {
			page: { type: Object as PropType<Page>, required: true },
			properties: {
				type: Object as PropType<Record<string, any> | null>,
				default: null,
			},
			directory: { type: Object as PropType<TmgrDirectory>, required: true },
			errors: { type: Object as PropType<PropertyErrors>, default: () => ({}) },
		},
		emits: ['update', 'navigate'],
		setup(props, { emit }) {
			const defs = computed(() => propertyDefsFor(props.page.type));
			const typeLabel = computed(
				() => TYPE_LABELS[props.page.type] ?? props.page.type,
			);
			const author = computed(() => toAuthorRef(props.page.author));
			const updatedBy = computed(() => toAuthorRef(props.page.updated_by));
			const updatedAt = computed(() =>
				props.page.updated_at
					? new Date(props.page.updated_at).toLocaleString()
					: '',
			);
			const current = computed(() => props.properties ?? props.page.properties);
			const valueOf = (key: string): any => current.value?.[key];

			const set = (key: string, value: unknown) =>
				emit('update', withProperty(current.value, key, value));
			const setText = (key: string, value: string) =>
				set(key, blankToNull(value));

			const refsOf = computed(() => {
				const refs: { kind: 'page' | 'user' | 'task'; id: string }[] = [];
				const user = valueOf('user_id');
				if (user) refs.push({ kind: 'user', id: String(user) });
				for (const item of valueOf('participants') ?? []) {
					const parsed = parseParticipant(item);
					if (parsed) refs.push(parsed);
				}
				for (const id of valueOf('related_tasks') ?? []) {
					refs.push({ kind: 'task', id: String(id) });
				}
				return refs;
			});

			watch(
				refsOf,
				(refs) => {
					if (refs.length) void props.directory.ensure(refs);
				},
				{ immediate: true },
			);

			const label = (kind: string, id: string) =>
				props.directory.titleFor(kind, id) ?? `#${id}`;

			const userChips = computed<ChipOption[]>(() => {
				const user = valueOf('user_id');
				return user
					? [{ key: String(user), label: label('user', String(user)) }]
					: [];
			});
			const participantChips = computed<ChipOption[]>(() =>
				(valueOf('participants') ?? []).map((item: string) => {
					const parsed = parseParticipant(item);
					return {
						key: item,
						label: parsed ? label(parsed.kind, parsed.id) : item,
					};
				}),
			);
			const taskChips = computed<ChipOption[]>(() =>
				(valueOf('related_tasks') ?? []).map((id: number) => ({
					key: String(id),
					label: label('task', String(id)),
				})),
			);

			const searchParticipants = async (query: string) =>
				(await props.directory.searchPeople(query)).map((item) => ({
					key: `tmgr://${item.kind}/${item.id}`,
					label: item.title,
					hint: item.kind === 'page' ? 'person' : 'member',
				}));
			const searchMembers = async (query: string) =>
				(await props.directory.searchMembers(query)).map((item) => ({
					key: item.id,
					label: item.title,
				}));
			const searchTasks = async (query: string) =>
				(await props.directory.searchTaskItems(query)).map((item) => ({
					key: item.id,
					label: item.title,
				}));

			const open = (kind: 'user' | 'task', id: string) =>
				emit('navigate', { form: 'storage', kind, id });
			const openParticipant = (key: string) => {
				const parsed = parseParticipant(key);
				if (parsed) emit('navigate', { form: 'storage', ...parsed });
			};

			return {
				defs,
				typeLabel,
				author,
				updatedBy,
				updatedAt,
				valueOf,
				set,
				setText,
				errorFor,
				userChips,
				participantChips,
				taskChips,
				searchParticipants,
				searchMembers,
				searchTasks,
				open,
				openParticipant,
			};
		},
	});
</script>
