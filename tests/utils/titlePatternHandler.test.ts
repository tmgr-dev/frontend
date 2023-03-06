import {expect, test} from '@jest/globals';
import { titlePatternHandler } from '../../src/utils/titlePatternHandler';
import moment from 'moment';

test('Title Pattern Handler', () => {
	expect(titlePatternHandler('TMGR-{index#workspace}: Test'))
		.toBe('TMGR-0: Test');
	expect(titlePatternHandler('TMGR-{index#category}: Test', new Map([['category', 1]])))
		.toBe('TMGR-1: Test');
	expect(titlePatternHandler('TMGR-{index#category}-{index#workspace}: Test', new Map([['category', 1]])))
		.toBe('TMGR-1-{index#workspace}: Test');
	expect(titlePatternHandler('TMGR-{index#category}-{index#workspace}: Test', new Map([['category', 1], ['workspace', 1]])))
		.toBe('TMGR-1-1: Test');

	const date = moment();
	expect(titlePatternHandler('TMGR-{index#category}-{index#workspace}: Hours - {dts#H#dte}; Minutes - {dts#S#dte}', new Map([['category', 1], ['workspace', 1]])))
		.toBe(`TMGR-1-1: Hours - ${date.format('H')}; Minutes - ${date.format('S')}`);
	expect(titlePatternHandler('TMGR-{index#category}-{index#workspace}: Hours - {dts#H#dte}; Minutes - {dts#S#dte}; Second hours: {dts#H#dte}', new Map([['category', 1], ['workspace', 1]])))
		.toBe(`TMGR-1-1: Hours - ${date.format('H')}; Minutes - ${date.format('S')}; Second hours: ${date.format('H')}`);

	expect(titlePatternHandler('TMGR-{index#workspace}: {dts#H:S:i#dte}'))
		.toBe(`TMGR-0: ${date.format('H:S:i')}`);
});
