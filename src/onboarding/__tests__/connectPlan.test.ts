import { planConnect } from '../connectPlan';

const deps = (over = {}) => ({
  getUser: jest.fn().mockResolvedValue({ has_smart_device_token: false }),
  fetchFacts: jest.fn().mockResolvedValue({}),
  ...over,
});

describe('planConnect', () => {
  it('mints for a user without a token and no earlier connection', async () => {
    const d = deps();
    expect(await planConnect(d)).toBe('mint');
    expect(d.fetchFacts).toHaveBeenCalledWith(['mcp_first_call_at']);
  });

  it('lets the user choose when a token already exists, without reading facts', async () => {
    const d = deps({ getUser: jest.fn().mockResolvedValue({ has_smart_device_token: true }) });
    expect(await planConnect(d)).toBe('choose');
    expect(d.fetchFacts).not.toHaveBeenCalled();
  });

  it('never mints when the user cannot be loaded', async () => {
    const d = deps({ getUser: jest.fn().mockRejectedValue(new Error('boom')) });
    expect(await planConnect(d)).toBe('user-error');
    expect(d.fetchFacts).not.toHaveBeenCalled();
  });

  it('offers an explicit mint when an agent connected before and no token exists', async () => {
    const d = deps({ fetchFacts: jest.fn().mockResolvedValue({ mcp_first_call_at: '2026-10-13T00:00:00Z' }) });
    expect(await planConnect(d)).toBe('reconnect');
  });

  it('mints when facts cannot be read, since the user has no token to lose', async () => {
    const d = deps({ fetchFacts: jest.fn().mockRejectedValue(new Error('down')) });
    expect(await planConnect(d)).toBe('mint');
  });
});
