import { Ok, Err, tryAsync } from 'wellcrafted/result';
import { WhisperingErr } from '$lib/result';
import { MeetingTranscriptionLive as service } from '$lib/services/meetings/transcription-live';
import { pinMeetingProvider } from './meeting-provider';
import { waitForCaptureStartup } from '$lib/services/capture-startup-live';
import { defineMutation, defineQuery, queryClient } from './_client';
export const meetingTranscription = {
	latest(id: string) {
		return defineQuery({
			queryKey: ['meetings', 'transcript', id],
			retry: false,
			queryFn: async () => {
				const r = await service.latest(id);
				if (r.error)
					return WhisperingErr({
						title: 'Cannot read local transcript',
						serviceError: r.error,
					});
				return Ok(r.data);
			},
		});
	},
	progress: defineQuery({
		queryKey: ['meetings', 'transcript-progress'],
		queryFn: () => Ok(service.progress()),
	}),
	run: defineMutation({
		mutationKey: ['meetings', 'transcribe-audio'],
		retry: false,
		mutationFn: async (p: {
			id: string;
			resume: boolean;
			confirmed: boolean;
		}) => {
			if (!p.confirmed)
				return WhisperingErr({
					title: 'Confirm audio-only transcription first',
				});
			const restored = await waitForCaptureStartup();
			if (restored.error)
				return WhisperingErr({
					title:
						'Capture restoration is not verified; transcription not started',
					serviceError: restored.error,
				});
			const configured = await tryAsync({
				try: pinMeetingProvider,
				catch: () =>
					WhisperingErr({ title: 'Cannot pin transcription configuration' }),
			});
			if (configured.error) return Err(configured.error);
			const c = configured.data;
			const r = await service.run(
				{
					...p,
					configurationId: c.configurationId,
					providerLabel: c.providerLabel,
				},
				c.transcribe,
			);
			if (r.error)
				return WhisperingErr({
					title: 'Transcription paused/failed; audio and saved text retained',
					serviceError: r.error,
				});
			return Ok(r.data);
		},
		onSettled: () =>
			queryClient.invalidateQueries({ queryKey: ['meetings', 'transcript'] }),
	}),
	pause: defineMutation({
		mutationKey: ['meetings', 'transcript-pause'],
		retry: false,
		mutationFn: async () => {
			service.pause();
			return Ok(undefined);
		},
	}),
	save: defineMutation({
		mutationKey: ['meetings', 'transcript-retry-save'],
		retry: false,
		mutationFn: async () => {
			const r = await service.retryPendingSave();
			if (r.error)
				return WhisperingErr({
					title: 'Local transcript save still failed',
					serviceError: r.error,
				});
			return Ok(r.data);
		},
		onSettled: () =>
			queryClient.invalidateQueries({ queryKey: ['meetings', 'transcript'] }),
	}),
};
