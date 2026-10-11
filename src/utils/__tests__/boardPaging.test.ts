import {
	appendUnknownTasks,
	columnOrders,
	consumeRest,
	restAfterPage,
} from '../boardPaging';

const task = (id: number, common_time = 0) => ({ id, common_time });
const noOvertime = () => 0;

describe('board paging', () => {
	it('renumbers the whole column once after a drop in a partially loaded column', () => {
		const total = Array.from({ length: 120 }, (_, i) => task(i + 1));
		const loaded = total.slice(0, 50);
		const dropped = loaded.splice(10, 1)[0];
		loaded.splice(3, 0, dropped);

		appendUnknownTasks(loaded, total);

		const orders = columnOrders(loaded);
		expect(orders).toHaveLength(120);
		expect(orders.map((o) => o.order)).toEqual(
			Array.from({ length: 120 }, (_, i) => i + 1),
		);
		expect(orders.slice(0, 5).map((o) => o.id)).toEqual([1, 2, 3, 11, 4]);
		expect(new Set(orders.map((o) => o.id)).size).toBe(120);
	});

	it('dedupes a task already shown by a realtime insert', () => {
		const column = [task(99), task(1), task(2)];
		appendUnknownTasks(column, [task(1), task(2), task(3), task(99)]);
		expect(column.map((t) => t.id)).toEqual([99, 1, 2, 3]);
	});

	it('keeps the header numbers of the unloaded remainder', () => {
		const first = [task(1, 100), task(2, 50)];
		const rest = restAfterPage(
			{ total: 5, total_seconds: 400, total_overtime_seconds: 30 },
			first,
			noOvertime,
		);
		expect(rest).toEqual({ count: 3, seconds: 250, overtime: 30 });

		consumeRest(rest, [task(3, 100), task(4, 100)], noOvertime);
		expect(rest).toEqual({ count: 1, seconds: 50, overtime: 30 });
	});
});
