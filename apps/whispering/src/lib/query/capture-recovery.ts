import { Ok } from 'wellcrafted/result';
import { WhisperingErr } from '$lib/result';
import { CaptureRecoveryServiceLive } from '$lib/services/capture-recovery-live';
import { defineMutation, defineQuery, queryClient } from './_client';

const statusKey = ['capture', 'saved-session-recovery'] as const;
export const captureRecovery = {
	getStatus: defineQuery({
		queryKey: statusKey,
		queryFn: async () => {
			const result = await CaptureRecoveryServiceLive.getStatus();
			if (result.error) return WhisperingErr({
				title: 'Cannot inspect saved recording metadata',
				serviceError: result.error,
			});
			return Ok(result.data);
		},
	}),
	recover: defineMutation({
		mutationKey: ['capture', 'recover-saved-session'],
		mutationFn: async () => {
			const result = await CaptureRecoveryServiceLive.recover();
			if (result.error) return WhisperingErr({
				title: 'Saved-session recovery did not complete',
				serviceError: result.error,
			});
			await queryClient.invalidateQueries({ queryKey: statusKey });
			return Ok(result.data);
		},
	}),
};
