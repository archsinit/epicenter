import { describe, expect, test } from 'bun:test';
import { automaticCleanupId } from './cleanup-policy';

describe('automatic cleanup consent', () => {
	for (const selectedId of [null, 'existing-cleanup']) {
		test(`disabled bypasses cleanup with selection ${selectedId}`, () => {
			expect(
				automaticCleanupId({
					'transformations.cleanupEnabled': false,
					'transformations.selectedTransformationId': selectedId,
				}),
			).toBeNull();
		});
	}

	test('enabled without a selection does not run an LLM', () => {
		expect(
			automaticCleanupId({
				'transformations.cleanupEnabled': true,
				'transformations.selectedTransformationId': null,
			}),
		).toBeNull();
	});

	test('enabled with a selection runs that transformation', () => {
		expect(
			automaticCleanupId({
				'transformations.cleanupEnabled': true,
				'transformations.selectedTransformationId': 'cleanup',
			}),
		).toBe('cleanup');
	});
});
