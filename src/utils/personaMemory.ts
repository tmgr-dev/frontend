import { byteLength } from '@/utils/personas';

export const SELF_MEMORY_MAX_BYTES = 4096;

export const selfMemoryTooLong = (body: string): boolean =>
	byteLength(body) > SELF_MEMORY_MAX_BYTES;

const pad = (n: number) => String(n).padStart(2, '0');

export const isCheckByStale = (
	checkBy: string | null | undefined,
	now: Date = new Date(),
): boolean => {
	if (!checkBy) return false;
	const today = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
	return checkBy < today;
};

export const totalTopicCount = (topics: Record<string, number>): number =>
	Object.values(topics).reduce((sum, n) => sum + n, 0);
