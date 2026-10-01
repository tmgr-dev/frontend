import { headingsMatch } from '../../utils/pages/headingAliases';
import { CHRONICLE_HEADING, MEETING, PERSON } from './markdown';

export const LAST_CONTACT_AT = 'last_contact_at';

const ALIAS_SOURCES = [
	'telegram',
	'rocketchat',
	'slack',
	'github',
	'email',
	'other',
];
const NETWORKS = ['operational', 'personal', 'strategic'];
const PARTICIPANT = /^tmgr:\/\/(page|user)\/\d+$/;
const MAX_ITEMS = 100;
const MAX_TEXT = 255;

type Props = Record<string, any>;
type Errors = Record<string, string>;

export class InvalidProperties extends Error {
	constructor(public errors: Errors) {
		super('Page properties are invalid');
	}
}

export const isTyped = (type: string): boolean =>
	type === PERSON || type === MEETING;

const isObject = (value: unknown): value is Props =>
	!!value && typeof value === 'object' && !Array.isArray(value);

const rejectUnknown = (input: Props, known: string[], errors: Errors) => {
	for (const key of Object.keys(input)) {
		if (!known.includes(key))
			errors[key] = 'is not a property of this page type';
	}
};

const integer = (value: unknown): number | null =>
	typeof value === 'number' && Number.isInteger(value) ? value : null;

const nullableInteger = (
	input: Props,
	key: string,
	errors: Errors,
): number | null => {
	const value = input[key];
	if (value === null || value === undefined) return null;
	const id = integer(value);
	if (id === null) errors[key] = 'must be an integer or null';
	return id;
};

const nullableText = (
	input: Props,
	key: string,
	errors: Errors,
): string | null => {
	const value = input[key];
	if (value === null || value === undefined) return null;
	if (typeof value !== 'string' || value.length > MAX_TEXT) {
		errors[key] = `must be a string of at most ${MAX_TEXT} characters or null`;
		return null;
	}
	return value;
};

const network = (value: unknown, errors: Errors): string | null => {
	if (value === null || value === undefined) return null;
	if (typeof value === 'string' && NETWORKS.includes(value)) return value;
	errors.network = 'must be one of operational, personal, strategic or null';
	return null;
};

const aliases = (value: unknown, errors: Errors): Props[] => {
	if (value === null || value === undefined) return [];
	if (!Array.isArray(value) || value.length > MAX_ITEMS) {
		errors.aliases = `must be an array of at most ${MAX_ITEMS} items`;
		return [];
	}
	const out: Props[] = [];
	value.forEach((alias, i) => {
		const at = `aliases[${i}]`;
		if (!isObject(alias)) {
			errors[at] = 'must be an object';
			return;
		}
		const { source, native_id: nativeId, display } = alias;
		if (typeof source !== 'string' || !ALIAS_SOURCES.includes(source)) {
			errors[`${at}.source`] =
				'must be one of telegram, rocketchat, slack, github, email, other';
		}
		if (
			typeof nativeId !== 'string' ||
			!nativeId.trim() ||
			nativeId.length > MAX_TEXT
		) {
			errors[
				`${at}.native_id`
			] = `must be a non-empty string of at most ${MAX_TEXT} characters`;
		}
		if (
			display !== null &&
			display !== undefined &&
			(typeof display !== 'string' || display.length > MAX_TEXT)
		) {
			errors[
				`${at}.display`
			] = `must be a string of at most ${MAX_TEXT} characters`;
		}
		for (const key of Object.keys(alias)) {
			if (!['source', 'native_id', 'display'].includes(key))
				errors[`${at}.${key}`] = 'is not a property of an alias';
		}
		out.push({ source, native_id: nativeId, display: display ?? '' });
	});
	return out;
};

const date = (value: unknown, errors: Errors): string | null => {
	if (value === null || value === undefined) return null;
	if (typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value)) {
		const parsed = new Date(`${value}T00:00:00Z`);
		if (
			!Number.isNaN(parsed.getTime()) &&
			parsed.toISOString().slice(0, 10) === value
		)
			return value;
	}
	errors.date = 'must be a date YYYY-MM-DD or null';
	return null;
};

const participants = (value: unknown, errors: Errors): string[] => {
	if (value === null || value === undefined) return [];
	if (!Array.isArray(value) || value.length > MAX_ITEMS) {
		errors.participants = `must be an array of at most ${MAX_ITEMS} tmgr://page/<id> or tmgr://user/<id> links`;
		return [];
	}
	const out: string[] = [];
	value.forEach((item, i) => {
		if (typeof item === 'string' && PARTICIPANT.test(item)) out.push(item);
		else
			errors[`participants[${i}]`] =
				'must be tmgr://page/<id> or tmgr://user/<id>';
	});
	return out;
};

const relatedTasks = (value: unknown, errors: Errors): number[] => {
	if (value === null || value === undefined) return [];
	if (!Array.isArray(value) || value.length > MAX_ITEMS) {
		errors.related_tasks = `must be an array of at most ${MAX_ITEMS} task ids`;
		return [];
	}
	const out: number[] = [];
	value.forEach((item, i) => {
		const id = integer(item);
		if (id === null || id <= 0)
			errors[`related_tasks[${i}]`] = 'must be a task id';
		else out.push(id);
	});
	return out;
};

const person = (
	input: Props,
	existing: Props | null,
	errors: Errors,
): Props => {
	rejectUnknown(
		input,
		['user_id', 'aliases', 'network', 'company', 'role', LAST_CONTACT_AT],
		errors,
	);
	return {
		user_id: nullableInteger(input, 'user_id', errors),
		aliases: aliases(input.aliases, errors),
		network: network(input.network, errors),
		company: nullableText(input, 'company', errors),
		role: nullableText(input, 'role', errors),
		[LAST_CONTACT_AT]: existing?.[LAST_CONTACT_AT] ?? null,
	};
};

const meeting = (input: Props, errors: Errors): Props => {
	rejectUnknown(input, ['date', 'participants', 'related_tasks'], errors);
	return {
		date: date(input.date, errors),
		participants: participants(input.participants, errors),
		related_tasks: relatedTasks(input.related_tasks, errors),
	};
};

/** The properties to store for `raw` on a page of `type`; throws InvalidProperties with a message per field. */
export const prepareProperties = (
	type: string,
	raw: unknown,
	existing: Props | null,
): Props => {
	if (raw !== null && raw !== undefined && !isObject(raw)) {
		throw new InvalidProperties({ properties: 'must be an object' });
	}
	const input: Props = (raw as Props | null | undefined) ?? {};
	if (!isTyped(type)) return input;
	const errors: Errors = {};
	const out =
		type === PERSON ? person(input, existing, errors) : meeting(input, errors);
	if (Object.keys(errors).length) throw new InvalidProperties(errors);
	return out;
};

export const isChronicleHeading = (
	heading: string | null | undefined,
): boolean => {
	let h = (heading ?? '').trim();
	while (h.startsWith('#')) h = h.substring(1);
	return headingsMatch(h, CHRONICLE_HEADING);
};
