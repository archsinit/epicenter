// Import all query modules

import { commands } from './actions';
import { db } from './db';
import { delivery } from './delivery';
import { download } from './download';
import { ffmpeg } from './ffmpeg';
import { notify } from './notify';
import { mouseShortcuts } from './mouse-shortcuts';
import { recorder } from './recorder';
import { shortcuts } from './shortcuts';
import { sound } from './sound';
import { text } from './text';
import { transcription } from './transcription';
import { transformer } from './transformer';
import { tray } from './tray';

/**
 * Unified namespace for all query operations.
 * Provides a single entry point for all TanStack Query-based operations.
 */
export const rpc = {
	mouseShortcuts,
	text,
	commands,
	db,
	download,
	ffmpeg,
	recorder,
	tray,
	shortcuts,
	sound,
	transcription,
	transformer,
	notify,
	delivery,
};
