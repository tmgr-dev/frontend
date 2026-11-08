import type {
	AliasSource,
	PageType,
	PersonaNetwork,
} from '@/actions/tmgr/pages';
import { t } from '@/i18n';

export type PropertyErrors = Record<string, string>;

export const NETWORK_OPTIONS: { value: PersonaNetwork; labelKey: string }[] = [
	{ value: 'operational', labelKey: 'pages.helpers.network.operational' },
	{ value: 'personal', labelKey: 'pages.helpers.network.personal' },
	{ value: 'strategic', labelKey: 'pages.helpers.network.strategic' },
];

export const ALIAS_SOURCES: { value: AliasSource; labelKey: string }[] = [
	{ value: 'telegram', labelKey: 'pages.helpers.aliasSource.telegram' },
	{ value: 'rocketchat', labelKey: 'pages.helpers.aliasSource.rocketchat' },
	{ value: 'slack', labelKey: 'pages.helpers.aliasSource.slack' },
	{ value: 'github', labelKey: 'pages.helpers.aliasSource.github' },
	{ value: 'email', labelKey: 'pages.helpers.aliasSource.email' },
	{ value: 'other', labelKey: 'pages.helpers.aliasSource.other' },
];

export const networkLabel = (value: string | null | undefined): string => {
	const option = NETWORK_OPTIONS.find((item) => item.value === value);
	return option ? t(option.labelKey) : '';
};

const DATE = /^\d{4}-\d{2}-\d{2}$/;
const PARTICIPANT = /^tmgr:\/\/(page|user)\/(\d+)$/;
const MAX_TEXT = 255;

const isDate = (value: unknown): boolean => {
	if (typeof value !== 'string' || !DATE.test(value)) return false;
	const parsed = new Date(`${value}T00:00:00Z`);
	return (
		!Number.isNaN(parsed.getTime()) &&
		parsed.toISOString().slice(0, 10) === value
	);
};

const isNullableText = (value: unknown): boolean =>
	value === null ||
	value === undefined ||
	(typeof value === 'string' && value.length <= MAX_TEXT);

const isId = (value: unknown): boolean =>
	typeof value === 'number' && Number.isInteger(value) && value > 0;

const isUserId = (value: unknown): boolean =>
	typeof value === 'number' && Number.isInteger(value) && value !== 0;

const validatePerson = (props: Record<string, any>): PropertyErrors => {
	const errors: PropertyErrors = {};
	const { user_id: userId, aliases, network, company, role } = props;
	if (userId !== null && userId !== undefined && !isUserId(userId)) {
		errors.user_id = t('pages.helpers.validation.invalidMember');
	}
	if (aliases !== undefined && aliases !== null) {
		if (!Array.isArray(aliases)) {
			errors.aliases = t('pages.helpers.validation.mustBeList');
		} else {
			const sources = ALIAS_SOURCES.map((option) => option.value);
			aliases.forEach((alias, index) => {
				if (!sources.includes(alias?.source)) {
					errors[`aliases.${index}.source`] = t(
						'pages.helpers.validation.chooseSource',
					);
				}
				if (
					typeof alias?.native_id !== 'string' ||
					!alias.native_id.trim() ||
					alias.native_id.length > MAX_TEXT
				) {
					errors[`aliases.${index}.native_id`] = t(
						'pages.helpers.validation.enterIdentifier',
					);
				}
				if (
					typeof alias?.display !== 'string' ||
					alias.display.length > MAX_TEXT
				) {
					errors[`aliases.${index}.display`] = t(
						'pages.helpers.validation.nameTooLong',
					);
				}
			});
		}
	}
	if (
		network !== null &&
		network !== undefined &&
		!NETWORK_OPTIONS.some((option) => option.value === network)
	) {
		errors.network = t('pages.helpers.validation.chooseRelationship');
	}
	if (!isNullableText(company))
		errors.company = t('pages.helpers.validation.maxChars');
	if (!isNullableText(role))
		errors.role = t('pages.helpers.validation.maxChars');
	return errors;
};

const validateMeeting = (props: Record<string, any>): PropertyErrors => {
	const errors: PropertyErrors = {};
	const { date, participants, related_tasks: relatedTasks } = props;
	if (date !== null && date !== undefined && !isDate(date)) {
		errors.date = t('pages.helpers.validation.dateFormat');
	}
	if (participants !== undefined && participants !== null) {
		if (!Array.isArray(participants)) {
			errors.participants = t('pages.helpers.validation.mustBeList');
		} else if (!participants.every((item) => PARTICIPANT.test(item))) {
			errors.participants = t('pages.helpers.validation.invalidMember');
		}
	}
	if (relatedTasks !== undefined && relatedTasks !== null) {
		if (!Array.isArray(relatedTasks) || !relatedTasks.every(isId)) {
			errors.related_tasks = t('pages.helpers.validation.invalidTask');
		}
	}
	return errors;
};

export const validateProperties = (
	type: PageType,
	props: Record<string, any> | null | undefined,
): PropertyErrors => {
	const value = props ?? {};
	if (type === 'person') return validatePerson(value);
	if (type === 'meeting') return validateMeeting(value);
	return {};
};

export const errorFor = (errors: PropertyErrors, key: string): string => {
	if (errors[key]) return errors[key];
	const nested = Object.keys(errors).find((name) => name.startsWith(`${key}.`));
	return nested ? errors[nested] : '';
};

const messageOf = (value: unknown): string => {
	if (Array.isArray(value))
		return value.map(messageOf).filter(Boolean)[0] ?? '';
	if (typeof value === 'string') return value;
	if (value && typeof value === 'object' && 'message' in value) {
		return String((value as { message: unknown }).message ?? '');
	}
	return '';
};

const normalizeField = (field: string): string =>
	field.replace(/^properties\./, '').replace(/\[(\d+)\]/g, '.$1');

export const parsePropertyErrors = (error: any): PropertyErrors | null => {
	const response = error?.response;
	if (response?.status !== 422) return null;
	const data = response.data;
	if (data?.error !== 'invalid_properties') return null;
	const source = data.errors ?? data.fields;
	const result: PropertyErrors = {};
	if (Array.isArray(source)) {
		for (const item of source) {
			const field = normalizeField(String(item?.field ?? item?.path ?? ''));
			const message = messageOf(item);
			if (field)
				result[field] = message || t('pages.helpers.validation.invalidValue');
		}
	} else if (source && typeof source === 'object') {
		for (const [field, value] of Object.entries(source)) {
			result[normalizeField(field)] =
				messageOf(value) || t('pages.helpers.validation.invalidValue');
		}
	}
	if (!Object.keys(result).length) {
		result._ =
			messageOf(data.message) ||
			t('pages.helpers.validation.invalidProperties');
	}
	return result;
};

export interface ParticipantRef {
	kind: 'page' | 'user';
	id: string;
}

export const parseParticipant = (value: string): ParticipantRef | null => {
	const match = PARTICIPANT.exec(value);
	return match ? { kind: match[1] as 'page' | 'user', id: match[2] } : null;
};

export const participantUrl = (kind: 'page' | 'user', id: string | number) =>
	`tmgr://${kind}/${id}`;

export const toggleItem = <T>(list: T[] | null | undefined, item: T): T[] => {
	const current = list ?? [];
	return current.includes(item)
		? current.filter((entry) => entry !== item)
		: [...current, item];
};

export const withProperty = (
	props: Record<string, any> | null | undefined,
	key: string,
	value: unknown,
): Record<string, any> => {
	const next: Record<string, any> = { ...(props ?? {}) };
	next[key] = Array.isArray(value) ? value.map(cloneEntry) : value;
	return next;
};

const cloneEntry = <T>(entry: T): T =>
	entry && typeof entry === 'object' ? { ...entry } : entry;

export const emptyAlias = () => ({
	source: 'telegram' as AliasSource,
	native_id: '',
	display: '',
});

export const blankToNull = (value: string): string | null => {
	const trimmed = value.trim();
	return trimmed ? trimmed : null;
};
