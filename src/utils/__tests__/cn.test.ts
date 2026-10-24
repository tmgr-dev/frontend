import { cn } from '@/utils';

describe('cn() with Tailwind 4 class names', () => {
	it.each([
		[['shadow-xs', 'shadow-sm'], 'shadow-sm'],
		[['shadow-sm', 'shadow-xs'], 'shadow-xs'],
		[['rounded-sm', 'rounded-md'], 'rounded-md'],
		[['outline-hidden', 'outline-none'], 'outline-none'],
		[['blur-xs', 'blur-sm'], 'blur-sm'],
		[['shrink-0', 'shrink'], 'shrink'],
		[['grow', 'grow-0'], 'grow-0'],
		[['h-(--radix-select-trigger-height)', 'h-4'], 'h-4'],
		[['bg-linear-to-r/srgb', 'bg-linear-to-l'], 'bg-linear-to-l'],
		[['-translate-x-1/2', 'translate-x-0'], 'translate-x-0'],
	])('cn(%j) -> %s', (input, expected) => {
		expect(cn(...input)).toBe(expected);
	});

	it('keeps non-conflicting v4 classes', () => {
		expect(cn('shadow-xs', 'outline-hidden', 'ring-3')).toBe(
			'shadow-xs outline-hidden ring-3',
		);
	});
});
