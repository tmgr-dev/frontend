import { availableTours } from '../whatsNewTours';

const map = { '0.9.37': ['a', 'b', 'c'], '0.9.36': ['d'] };

describe('availableTours', () => {
  it('returns the tours of the version that are available', () => {
    const status = (id: string) => (id === 'b' ? 'completed' : id === 'c' ? 'unavailable' : 'available');
    expect(availableTours('0.9.37', status, map)).toEqual(['a']);
  });

  it('hides completed tours', () => {
    expect(availableTours('0.9.36', () => 'completed', map)).toEqual([]);
  });

  it('returns nothing for a version without tours', () => {
    expect(availableTours('1.0.0', () => 'available', map)).toEqual([]);
  });
});
