import { Ok } from 'wellcrafted/result';
import { WhisperingErr } from '$lib/result';
import { MeetingRecoveryLive as service } from '$lib/services/meetings/recovery-live';
import { defineMutation, defineQuery, queryClient } from './_client';
const key = ['meetings', 'recovery'] as const;
let last = '';
const refresh = () => queryClient.invalidateQueries({ queryKey: key });
export const meetingRecovery = {
	status: defineQuery({
		queryKey: key,
		retry: false,
		queryFn: async () => {
			const r = await service.status();
			if (r.error)
				return WhisperingErr({
					title: 'Recovery state is unknown',
					serviceError: r.error,
				});
			const fingerprint = r.data ? r.data.token + '/' + r.data.phase : '';
			if (fingerprint !== last) {
				last = fingerprint;
				await queryClient.invalidateQueries({
					queryKey: ['meetings', 'library'],
				});
				await queryClient.invalidateQueries({
					queryKey: ['meetings', 'detail'],
				});
			}
			return Ok(r.data);
		},
	}),
	prepare: defineMutation({
		mutationKey: [...key, 'prepare'],
		retry: false,
		mutationFn: async (id: string) => {
			const r = await service.prepare(id);
			if (r.error)
				return WhisperingErr({
					title: 'Recovery review was not acknowledged',
					serviceError: r.error,
				});
			return Ok(r.data);
		},
		onSettled: refresh,
	}),
	execute: defineMutation({
		mutationKey: [...key, 'execute'],
		retry: false,
		mutationFn: async (p: { token: string; confirmed: boolean }) => {
			const r = await service.execute(p.token, p.confirmed);
			if (r.error)
				return WhisperingErr({
					title: 'Recovery acknowledgement unavailable; observe native status',
					serviceError: r.error,
				});
			return Ok(r.data);
		},
		onSettled: refresh,
	}),
	cancel: defineMutation({
		mutationKey: [...key, 'cancel'],
		retry: false,
		mutationFn: async (token: string) => {
			const r = await service.cancel(token);
			if (r.error)
				return WhisperingErr({
					title: 'Review cancellation was not acknowledged',
					serviceError: r.error,
				});
			return Ok(r.data);
		},
		onSettled: refresh,
	}),
};
