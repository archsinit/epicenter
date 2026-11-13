import { extractErrorMessage } from 'wellcrafted/error';
import { Err, Ok, tryAsync, trySync } from 'wellcrafted/result';

/** Attempt every track even when one fails. A non-ended track is not closed. */
export function stopRecordingStream(stream: MediaStream) {
	const failures: string[] = [];
	for (const track of stream.getTracks()) {
		const stopped = trySync({
			try: () => track.stop(),
			catch: (error) => Err(extractErrorMessage(error)),
		});
		if (stopped.error) failures.push(stopped.error);
		if (track.readyState !== 'ended') failures.push('A microphone track has not ended.');
	}
	if (failures.length) throw new Error(failures.join(' '));
}

/** Each consumer owns its temporary permission/enumeration streams. No stream
 * is forgotten if enumeration or track cleanup fails. Admission belongs to the
 * caller (recorder/VAD); fallback enumeration must not acquire a nested lease.
 */
export function createDeviceEnumerationCore({
	acquire,
	enumerate,
	cleanupStream,
}: {
	acquire: () => Promise<MediaStream>;
	enumerate: () => Promise<MediaDeviceInfo[]>;
	cleanupStream: (stream: MediaStream) => void;
}) {
	const pending = new Set<MediaStream>();
	let busy = false;
	function cleanup() {
		const failures: string[] = [];
		for (const stream of pending) {
			const result = trySync({
				try: () => cleanupStream(stream),
				catch: (error) => Err(extractErrorMessage(error)),
			});
			if (result.error) failures.push(result.error);
			else pending.delete(stream);
		}
		if (failures.length) throw new Error(failures.join(' '));
	}
	return {
		get held() { return busy || pending.size > 0; },
		cleanup,
		async enumerate() {
			if (busy) return Err({ message: 'Device enumeration is already pending.' });
			busy = true;
			const result = await tryAsync({
				try: async () => {
					cleanup();
					pending.add(await acquire());
					return enumerate();
				},
				catch: (error) => Err({ message: extractErrorMessage(error) }),
			});
			const closed = trySync({
				try: cleanup,
				catch: (error) => Err({ message: extractErrorMessage(error) }),
			});
			busy = false;
			if (closed.error) return Err(closed.error);
			return result.error ? Err(result.error) : Ok(result.data);
		},
	};
}
