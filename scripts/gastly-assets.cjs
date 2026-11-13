// Offline asset preparation only. Requires sharp (not an app/runtime dependency).
// NODE_PATH=/path/to/node_modules node scripts/gastly-assets.cjs
const fs = require('node:fs');
const path = require('node:path');
const sharp = require('sharp');
const root = path.resolve(__dirname, '../apps/whispering/static/companion');
const source = path.join(root, 'approved-reference.png');

async function extract(name, crop, mask) {
	const { data, info } = await sharp(source).extract(crop).ensureAlpha().raw()
		.toBuffer({ resolveWithObject: true });
	for (let y = 0; y < info.height; y++) {
		for (let x = 0; x < info.width; x++) {
			const a = (y * info.width + x) * 4 + 3;
			data[a] = Math.round(data[a] * mask(x, y));
		}
	}
	await sharp(data, { raw: info }).png().toFile(path.join(root, name));
}
const clamp = (v) => Math.max(0, Math.min(1, v));
const smooth = (v) => { const t = clamp(v); return t * t * (3 - 2 * t); };

(async () => {
	fs.mkdirSync(root, { recursive: true });
	// Coordinates are tied to Roger's unscaled 1536×1024 approved original.
	const idle = { left: 180, top: 0, width: 576, height: 512 };
	await extract('idle-smoke.png', idle, (x, y) => {
		const r = Math.hypot(x - 287, y - 253);
		// Isolate each form and remove the face from the smoke layer.
		return smooth((r - 76) / 7) * smooth((253 - r) / 67);
	});
	await extract('idle-head.png', { left: 383, top: 169, width: 168, height: 168 },
		(x, y) => smooth((84 - Math.hypot(x - 84, y - 84)) / 5));
	await extract('happy-head.png', { left: 969, top: 650, width: 152, height: 152 },
		(x, y) => smooth((76 - Math.hypot(x - 76, y - 76)) / 5));
	await extract('vortex.png', { left: 180, top: 512, width: 576, height: 512 },
		(x, y) => smooth((250 - Math.hypot(x - 288, y - 234)) / 65));
	console.log('Prepared companion textures; approved-reference.png unchanged.');
})().catch((error) => { console.error(error.message); process.exitCode = 1; });
