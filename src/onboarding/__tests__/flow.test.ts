import { consentStepReady, steps } from '../flow';

describe('flow.steps', () => {
  it('consent is not ready yet', () => {
    expect(consentStepReady()).toBe(false);
  });

  it.each([
    [null, ['intent']],
    ['human', ['intent']],
    ['skipped', ['intent']],
    ['agent', ['intent', 'connect']],
  ] as const)('steps(%p)', (intent, expected) => {
    expect(steps(intent)).toEqual(expected);
  });
});
