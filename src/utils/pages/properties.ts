import type {
	AliasSource,
	PageType,
	PersonaNetwork,
} from '@/actions/tmgr/pages';

export type PropertyErrors = Record<string, string>;

export const NETWORK_OPTIONS: { value: PersonaNetwork; label: string }[] = [
	{ value: 'operational', label: 'Operational' },
	{ value: 'personal', label: 'Personal' },
	{ value: 'strategic', label: 'Strategic' },
];

export const ALIAS_SOURCES: { value: AliasSource; label: string }[] = [
	{ value: 'telegram', label: 'Telegram' },
	{ value: 'rocketchat', label: 'Rocket.Chat' },
	{ value: 'slack', label: 'Slack' },
	{ value: 'github', label: 'GitHub' },
	{ value: 'email', label: 'Email' },
	{ value: 'other', label: 'Other' },
];

export const networkLabel = (value: string | null | undefined): string =>
	NETWORK_OPTIONS.find((option) => option.value === value)?.label ?? '';

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

const validatePerson = (props: Record<string, any>): PropertyErrors => {
	const errors: PropertyErrors = {};
	const { user_id: userId, aliases, network, company, role } = props;
	if (userId !== null && userId !== undefined && !isId(userId)) {
		errors.user_id = 'Invalid member';
	}
	if (aliases !== undefined && aliases !== null) {
		if (!Array.isArray(aliases)) {
			errors.aliases = 'Must be a list';
		} else {
			const sources = ALIAS_SOURCES.map((option) => option.value);
			aliases.forEach((alias, index) => {
				if (!sources.includes(alias?.source)) {
					errors[`aliases.${index}.source`] = 'Choose a source';
				}
				if (
					typeof alias?.native_id !== 'string' ||
					!alias.native_id.trim() ||
					alias.native_id.length > MAX_TEXT
				) {
					errors[`aliases.${index}.native_id`] = 'Enter an identifier';
				}
				if (
					typeof alias?.display !== 'string' ||
					alias.display.length > MAX_TEXT
				) {
					errors[`aliases.${index}.display`] = 'Name is too long';
				}
			});
		}
	}
	if (
		network !== null &&
		network !== undefined &&
		!NETWORK_OPTIONS.some((option) => option.value === network)
	) {
		errors.network = 'Choose a relationship type';
	}
	if (!isNullableText(company)) errors.company = 'At most 255 characters';
	if (!isNullableText(role)) errors.role = 'At most 255 characters';
	return errors;
};

const validateMeeting = (props: Record<string, any>): PropertyErrors => {
	const errors: PropertyErrors = {};
	const { date, participants, related_tasks: relatedTasks } = props;
	if (date !== null && date !== undefined && !isDate(date)) {
		errors.date = 'Date in YYYY-MM-DD format';
	}
	if (participants !== undefined && participants !== null) {
		if (!Array.isArray(participants)) {
			errors.participants = 'Must be a list';
		} else if (!participants.every((item) => PARTICIPANT.test(item))) {
			errors.participants = 'Invalid member';
		}
	}
	if (relatedTasks !== undefined && relatedTasks !== null) {
		if (!Array.isArray(relatedTasks) || !relatedTasks.every(isId)) {
			errors.related_tasks = 'Invalid task';
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
			if (field) result[field] = message || 'Invalid value';
		}
	} else if (source && typeof source === 'object') {
		for (const [field, value] of Object.entries(source)) {
			result[normalizeField(field)] =
				messageOf(value) || 'Invalid value';
		}
	}
	if (!Object.keys(result).length) {
		result._ = messageOf(data.message) || 'Invalid properties';
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
