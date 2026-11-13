import { Ok } from 'wellcrafted/result';
import { WhisperingErr } from '$lib/result';
import { MeetingControlsServiceLive as service } from '$lib/services/meetings/controls-live';
import type { StartMeetingParams } from '$lib/services/meetings/controls';
import { defineMutation, defineQuery, queryClient } from './_client';

const statusKey = ['meetings', 'native-status'] as const;
let lastLibrarySnapshot = '';
const refreshStatus = () => queryClient.invalidateQueries({ queryKey: statusKey });
export const meetingControls = {
	status: defineQuery({
		queryKey: statusKey,
		retry: false,
		queryFn: async () => {
			const result = await service.getStatus();
			if (result.error) return WhisperingErr({ title: 'Meeting capture state is unknown', serviceError: result.error });
			const p = result.data.progress;
			if (p && lastLibrarySnapshot !== `${p.id}/${p.phase}/${p.captureReserved}/${p.screenshots}`) {
				lastLibrarySnapshot = `${p.id}/${p.phase}/${p.captureReserved}/${p.screenshots}`;
				await queryClient.invalidateQueries({ queryKey: ['meetings', 'library'] });
				await queryClient.invalidateQueries({ queryKey: ['meetings', 'detail', p.id] });
			}
			return Ok(result.data);
		},
	}),
	loadSources: defineMutation({
		mutationKey: ['meetings', 'load-sources'],
		retry: false,
		mutationFn: async () => {
			const result = await service.loadSources();
			if (result.error) return WhisperingErr({ title: 'Cannot load meeting sources', serviceError: result.error });
			return Ok(result.data);
		},
	}),
	permission: defineMutation({
		mutationKey: ['meetings', 'explicit-permission'],
		retry: false,
		mutationFn: async (kind: 'microphone' | 'screen') => {
			const result = await service.requestPermission(kind);
			if (result.error) return WhisperingErr({ title: 'Meeting permission request did not complete', serviceError: result.error });
			return Ok({ kind, granted: result.data });
		},
	}),
	start: defineMutation({
		mutationKey: ['meetings', 'start'],
		retry: false,
		mutationFn: async (params: StartMeetingParams) => {
			const result = await service.start(params);
			if (result.error) return WhisperingErr({ title: 'Meeting Start was not confirmed; check status or request Stop', serviceError: result.error });
			return Ok(result.data);
		},
		onSettled: refreshStatus,
	}),
	stop: defineMutation({
		mutationKey: ['meetings', 'stop'],
		retry: false,
		mutationFn: async () => {
			const result = await service.stop();
			if (result.error) return WhisperingErr({ title: 'Meeting Stop/finalization is not confirmed', serviceError: result.error });
			queryClient.setQueryData(statusKey, result.data);
			return Ok(result.data);
		},
		onSettled: refreshStatus,
	}),
	captureNow: defineMutation({
		mutationKey: ['meetings', 'capture-now'],
		retry: false,
		mutationFn: async () => {
			const result = await service.captureNow();
			if (result.error) return WhisperingErr({ title: 'Screenshot request was not acknowledged', serviceError: result.error });
			return Ok(undefined);
		},
		onSettled: refreshStatus,
	}),
	pauseScreenshots: defineMutation({
		mutationKey: ['meetings', 'pause-screenshots'],
		retry: false,
		mutationFn: async (paused: boolean) => {
			const result = await service.pauseScreenshots(paused);
			if (result.error) return WhisperingErr({ title: 'Screenshot pause request was not acknowledged', serviceError: result.error });
			return Ok(undefined);
		},
		onSettled: refreshStatus,
	}),
};
