import { afterPick, folderLine, NO_FOLDER_TEXT } from '../folder';

describe('folderLine', () => {
	it('shows the chosen path with a title', () => {
		expect(folderLine('/work/app')).toEqual({
			chosen: true,
			text: '/work/app',
			title: '/work/app',
		});
	});

	it('explains that the folder is picked on run', () => {
		expect(folderLine(null)).toEqual({
			chosen: false,
			text: NO_FOLDER_TEXT,
			title: undefined,
		});
		expect(NO_FOLDER_TEXT).toBe(
			'Not chosen yet — you will pick it when you run',
		);
	});
});

describe('afterPick', () => {
	it('keeps the old value when the picker was cancelled', () => {
		expect(afterPick('/old', null)).toBe('/old');
		expect(afterPick(null, null)).toBeNull();
	});

	it('takes the new folder', () => {
		expect(afterPick('/old', '/new')).toBe('/new');
	});
});
