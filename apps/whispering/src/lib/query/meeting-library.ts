import { Ok } from 'wellcrafted/result';
import { WhisperingErr } from '$lib/result';
import { MeetingLibraryServiceLive } from '$lib/services/meetings/library-live';
import { defineMutation, defineQuery } from './_client';

export const meetingLibrary = {
	list: defineQuery({
		queryKey: ['meetings', 'library'],
		queryFn: async () => {
			const result = await MeetingLibraryServiceLive.list();
			if (result.error) return WhisperingErr({ title: 'Cannot read local meetings', serviceError: result.error });
			return Ok(result.data);
		},
	}),
	detail(id: string) {
		return defineQuery({
			queryKey: ['meetings', 'detail', id],
			queryFn: async () => {
				const result = await MeetingLibraryServiceLive.detail(id);
				if (result.error) return WhisperingErr({ title: 'Cannot read this local meeting', serviceError: result.error });
				return Ok(result.data);
			},
		});
	},
	revealFolder: defineMutation({
        mutationKey: ['meetings', 'reveal-folder'],
        retry: false,
        mutationFn: async (id: string) => {
            const result = await MeetingLibraryServiceLive.revealFolder(id);
            if (result.error) return WhisperingErr({ title: 'Cannot reveal local meeting folder', serviceError: result.error });
            return Ok(result.data);
        },
    }),
	preview: defineMutation({
		mutationKey: ['meetings', 'local-preview'],
		mutationFn: async ({ id, index, kind }: { id: string; index: number; kind: 'audio' | 'local-screenshot' }) => {
			const result = await (kind === 'audio'
				? MeetingLibraryServiceLive.readMixedAudio(id, index)
				: MeetingLibraryServiceLive.readScreenshot(id, index));
			if (result.error) return WhisperingErr({ title: 'Cannot open local reference', serviceError: result.error });
			return Ok({ id, index, ...result.data });
		},
	}),
};
