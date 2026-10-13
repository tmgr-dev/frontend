export const NO_FOLDER_TEXT = 'Not chosen yet — you will pick it when you run';

export interface FolderLine {
	chosen: boolean;
	text: string;
	title: string | undefined;
}

export const folderLine = (path: string | null): FolderLine =>
	path
		? { chosen: true, text: path, title: path }
		: { chosen: false, text: NO_FOLDER_TEXT, title: undefined };

export const afterPick = (
	current: string | null,
	picked: string | null,
): string | null => picked ?? current;
