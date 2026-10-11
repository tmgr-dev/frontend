import $axios from '@/plugins/axios';
import { getPromotions, sendPromotionEvent } from '../promotions';

jest.mock('@/plugins/axios', () => ({
	__esModule: true,
	default: { get: jest.fn(), post: jest.fn() },
}));

describe('promotions api', () => {
	afterEach(() => jest.clearAllMocks());

	it('requests promotions for a placement and locale', async () => {
		($axios.get as jest.Mock).mockResolvedValue({
			data: { data: [{ id: 1 }] },
		});
		expect(await getPromotions('about', 'de-DE')).toEqual([{ id: 1 }]);
		expect($axios.get).toHaveBeenCalledWith('promotions', {
			params: { placement: 'about', locale: 'de-DE' },
		});
	});

	it('tolerates a malformed payload', async () => {
		($axios.get as jest.Mock).mockResolvedValue({ data: {} });
		expect(await getPromotions('about', 'en')).toEqual([]);
	});

	it('posts an event with the contract body', async () => {
		($axios.post as jest.Mock).mockResolvedValue({});
		await sendPromotionEvent(3, 'release_notes', 'click');
		expect($axios.post).toHaveBeenCalledWith('promotions/events', {
			campaign_id: 3,
			placement: 'release_notes',
			type: 'click',
		});
	});
});
