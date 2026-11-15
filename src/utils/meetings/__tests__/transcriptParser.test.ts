import {
	formatTimecode,
	parseTranscript,
	segmentAt,
} from '../transcriptParser';

describe('formatTimecode', () => {
	it('uses mm:ss below an hour and h:mm:ss above', () => {
		expect(formatTimecode(0)).toBe('00:00');
		expect(formatTimecode(754_000)).toBe('12:34');
		expect(formatTimecode(3_723_000)).toBe('1:02:03');
	});
});

describe('parseTranscript', () => {
	it('reads bracketed and bare timecodes with speakers', () => {
		const text = [
			'[00:05] Anna: Let us start',
			'12:34 Bob: I will fix the login',
		].join('\n');

		const result = parseTranscript(text);

		expect(result.hasTimecodes).toBe(true);
		expect(result.segments).toEqual([
			{ startMs: 5_000, speaker: 'Anna', text: 'Let us start' },
			{ startMs: 754_000, speaker: 'Bob', text: 'I will fix the login' },
		]);
	});

	it('reads h:mm:ss with fractions', () => {
		const result = parseTranscript('(1:02:03.500) We ship on Friday');

		expect(result.segments[0].startMs).toBe(3_723_500);
		expect(result.segments[0].text).toBe('We ship on Friday');
	});

	it('appends lines without a timecode to the previous segment', () => {
		const result = parseTranscript(
			'[00:10] Anna: first line\nsecond line\n\n[00:20] Bob: next',
		);

		expect(result.segments).toHaveLength(2);
		expect(result.segments[0].text).toBe('first line second line');
	});

	it('parses SRT cues', () => {
		const srt = [
			'1',
			'00:00:01,000 --> 00:00:04,000',
			'Hello everyone',
			'',
			'2',
			'00:01:05,250 --> 00:01:08,000',
			'We need a release',
			'checklist',
		].join('\n');

		const result = parseTranscript(srt);

		expect(result.segments).toEqual([
			{ startMs: 1_000, endMs: 4_000, text: 'Hello everyone' },
			{ startMs: 65_250, endMs: 68_000, text: 'We need a release checklist' },
		]);
	});

	it('parses WebVTT cues with optional hours and tags', () => {
		const vtt = [
			'WEBVTT',
			'',
			'NOTE a comment',
			'',
			'00:01.500 --> 00:03.000',
			'<v Anna>Plan the sprint</v>',
		].join('\n');

		const result = parseTranscript(vtt);

		expect(result.segments).toEqual([
			{
				startMs: 1_500,
				endMs: 3_000,
				speaker: 'Anna',
				text: 'Plan the sprint',
			},
		]);
	});

	it('falls back to one untimed segment when there are no timecodes', () => {
		const result = parseTranscript('We talked about the budget.\nAnd hiring.');

		expect(result.hasTimecodes).toBe(false);
		expect(result.segments).toEqual([
			{ startMs: null, text: 'We talked about the budget. And hiring.' },
		]);
	});

	it('returns nothing for blank input and strips a BOM and CRLF', () => {
		expect(parseTranscript('  \n ').segments).toEqual([]);
		expect(parseTranscript('﻿[00:01] a\r\n[00:02] b').segments).toHaveLength(2);
	});

	it('does not read a clock time inside a sentence as a timecode', () => {
		const result = parseTranscript('[00:10] Anna: meet at 10:30 tomorrow');

		expect(result.segments).toHaveLength(1);
		expect(result.segments[0].text).toBe('meet at 10:30 tomorrow');
	});

	it('keeps segments ordered by start and ignores non-increasing garbage order for lookup', () => {
		const result = parseTranscript('[00:30] b\n[00:10] a');

		expect(result.segments.map((s) => s.startMs)).toEqual([10_000, 30_000]);
	});
});

describe('segmentAt', () => {
	it('finds the segment that contains a time', () => {
		const { segments } = parseTranscript('[00:10] a\n[00:30] b\n[01:00] c');

		expect(segmentAt(segments, 45_000)?.text).toBe('b');
		expect(segmentAt(segments, 5_000)).toBeUndefined();
	});
});
