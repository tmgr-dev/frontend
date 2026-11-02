import { readFileSync } from 'fs';
import { resolve } from 'path';

const source = readFileSync(resolve(__dirname, '../Board.vue'), 'utf8');

describe('Board column footer', () => {
	it('renders the sentinel and quick-add after the cards, so later pages land above them', () => {
		const footer = source.slice(
			source.indexOf('<template #footer>'),
			source.indexOf('</Draggable>'),
		);
		expect(footer).toContain('<ColumnSentinel');
		expect(footer).toContain('board-add-task');
		expect(footer).toContain('task-title-input');
	});

	it('teleports nothing into the column scroller, where new cards would land below it', () => {
		expect(source).not.toMatch(/:to="`\.board-card-draggable/);
	});
});
