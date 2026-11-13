/** Small, alpha-composited procedural volume. No smoke/vortex image is animated. */
export const VERTEX_SHADER = `
attribute vec2 position;
varying vec2 uv;
void main() { uv = position * 0.5 + 0.5; gl_Position = vec4(position, 0.0, 1.0); }
`;

export const FRAGMENT_SHADER = `
precision highp float;
varying vec2 uv;
uniform float time;
uniform float wave;
uniform float processing;
uniform float yaw;
uniform float pitch;
uniform float blink;
uniform float failure;
uniform vec2 resolution;
uniform sampler2D face;

float hash(vec3 p) {
    p = fract(p * 0.1031);
    p += dot(p, p.yzx + 33.33);
    return fract((p.x + p.y) * p.z);
}
float noise(vec3 p) {
    vec3 i = floor(p), f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(mix(hash(i), hash(i + vec3(1,0,0)), f.x),
                   mix(hash(i + vec3(0,1,0)), hash(i + vec3(1,1,0)), f.x), f.y),
               mix(mix(hash(i + vec3(0,0,1)), hash(i + vec3(1,0,1)), f.x),
                   mix(hash(i + vec3(0,1,1)), hash(i + vec3(1,1,1)), f.x), f.y), f.z);
}
float fbm(vec3 p) {
    float v = 0.0, a = 0.57;
    for (int i = 0; i < 3; i++) {
        v += a * noise(p);
        p = p * 2.07 + vec3(5.2, 1.3, 7.1);
        a *= 0.48;
    }
    return v;
}
mat2 rotation(float a) { float c = cos(a), s = sin(a); return mat2(c, -s, s, c); }
vec3 turnY(vec3 p, float a) { return vec3(cos(a)*p.x + sin(a)*p.z, p.y, -sin(a)*p.x + cos(a)*p.z); }
vec3 turnX(vec3 p, float a) { return vec3(p.x, cos(a)*p.y - sin(a)*p.z, sin(a)*p.y + cos(a)*p.z); }

void main() {
    vec2 p = (uv - 0.5) * vec2(3.0, 2.0);
    p.y -= 0.125;
    float headAmount = max(0.0, 1.0 - wave - processing);
    vec2 hp = p - vec2(0.0, sin(time * 1.2) * 0.025 * headAmount);
    float radius = 0.292;
    float r2 = dot(hp, hp) / (radius * radius);
    float ha = (1.0 - smoothstep(0.96, 1.0, r2)) * headAmount;
    float hz = sqrt(max(0.0, 1.0 - r2)) * radius;
    vec3 normal = vec3(hp / radius, sqrt(max(0.0, 1.0 - r2)));
    vec3 local = turnX(turnY(normal, -yaw), -pitch);
    vec2 faceUV = local.xy * 0.5 + 0.5;
    vec4 artwork = texture2D(face, clamp(faceUV, 0.0, 1.0));
    vec3 material = vec3(0.105, 0.07, 0.15) * (0.85 + 0.15 * normal.z);
    vec3 head = mix(material, artwork.rgb, artwork.a * smoothstep(-0.03, 0.12, local.z));
    // Fade the white eye regions closed in face-local space: they turn with the sphere.
    float eyeRegion = step(0.30, faceUV.y) * step(faceUV.y, 0.65)
        * (1.0 - step(0.46, faceUV.x) * step(faceUV.x, 0.54));
    float whites = smoothstep(0.48, 0.85, min(artwork.r, min(artwork.g, artwork.b)));
    head = mix(head, vec3(0.13, 0.085, 0.19), blink * eyeRegion * whites);
    head += vec3(0.11, 0.04, 0.17) * pow(1.0 - clamp(normal.z, 0.0, 1.0), 3.0);

    vec3 color = vec3(0.0);
    float alpha = 0.0;
    bool headPlaced = false;
    for (int i = 0; i < 8; i++) {
        float z = 1.15 - float(i) * 0.32;
        if (!headPlaced && z < hz) {
            color += (1.0 - alpha) * head * ha;
            alpha += (1.0 - alpha) * ha;
            headPlaced = true;
        }
        vec3 q = vec3(p.x * 1.45 / mix(1.0, 1.43, wave), p.y * 1.45 / mix(1.0, 0.43, wave), z);
        float r = length(q.xy);
        // Angular speed varies with radius/depth. The noise also advects and deforms:
        // this is deliberately NOT a rigidly rotated noise field or texture.
        q.xy = rotation(processing * (2.7 * exp(-r * 1.4) - time * 0.52 + z * 0.5)) * q.xy;
        vec3 advect = vec3(time * 0.11, -time * 0.16, time * 0.13);
        vec3 field = q * vec3(3.8, 3.8, 2.6);
        vec3 warp = vec3(
            noise(field * 0.64 + advect),
            noise(field * 0.71 - advect.yzx + 7.3),
            noise(field * 0.59 + advect.zxy + 12.1));
        float n = fbm(field + (warp - 0.5) * 2.4 + advect);
        float fine = noise(field * 3.1 - advect * 1.7);
        float edge = 1.0 - smoothstep(0.72, 1.17, r + (warp.x - 0.5) * 0.25);
        float volume = exp(-dot(q * vec3(0.86, 0.86, 1.0), q * vec3(0.86, 0.86, 1.0)) * 1.65);
        float density = smoothstep(0.25, 0.65, n + fine * 0.13) * edge * volume * 0.48;
        // A soft vortex hollow; turbulence is still visible inside and across its rim.
        density *= mix(1.0, smoothstep(0.08, 0.42, r), processing * 0.88);
        // Clear facial features without cutting a hole through the background gas.
        if (z > hz) density *= mix(0.16, 1.0, smoothstep(0.24, 0.48, length(hp)) * headAmount + (1.0 - headAmount));
        float filament = pow(max(0.0, 1.0 - abs(n - 0.53) * 5.0), 3.0);
        vec3 violet = mix(vec3(0.22, 0.035, 0.39), vec3(0.65, 0.32, 0.88), n);
        violet = mix(violet, vec3(0.88, 0.67, 1.0), filament * (0.22 + fine * 0.25));
        violet = mix(violet, vec3(0.75, 0.18, 0.40), failure * 0.25);
        color += (1.0 - alpha) * violet * density;
        alpha += (1.0 - alpha) * density;
    }
    if (!headPlaced) { color += (1.0 - alpha) * head * ha; alpha += (1.0 - alpha) * ha; }
    // Explicit transparent border prevents clipping/raster seams at native bounds.
    float border = smoothstep(0.0, 0.035, min(min(uv.x, 1.0-uv.x), min(uv.y, 1.0-uv.y)));
    gl_FragColor = vec4(color * border, alpha * border); // premultiplied alpha
}
`;

export type CompanionFrame = {
	time: number;
	wave: number;
	processing: number;
	yaw: number;
	pitch: number;
	blink: number;
	failure: number;
};

export function createCompanionRenderer(canvas: HTMLCanvasElement) {
	const gl = canvas.getContext('webgl', {
		alpha: true, premultipliedAlpha: true, antialias: false,
		depth: false, stencil: false, powerPreference: 'low-power',
	});
	if (!gl) return;
	const vertex = gl.createShader(gl.VERTEX_SHADER);
	const fragment = gl.createShader(gl.FRAGMENT_SHADER);
	const program = gl.createProgram();
	const buffer = gl.createBuffer();
	const texture = gl.createTexture();
	function dispose() {
		image.onload = null;
		gl?.deleteTexture(texture); gl?.deleteBuffer(buffer);
		gl?.deleteProgram(program); gl?.deleteShader(vertex); gl?.deleteShader(fragment);
	}
	const image = new Image();
	if (!vertex || !fragment || !program || !buffer || !texture) { dispose(); return; }
	gl.shaderSource(vertex, VERTEX_SHADER); gl.compileShader(vertex);
	gl.shaderSource(fragment, FRAGMENT_SHADER); gl.compileShader(fragment);
	if (!gl.getShaderParameter(vertex, gl.COMPILE_STATUS) || !gl.getShaderParameter(fragment, gl.COMPILE_STATUS)) {
		console.warn('Companion shader unavailable:', gl.getShaderInfoLog(fragment));
		dispose(); return;
	}
	gl.attachShader(program, vertex); gl.attachShader(program, fragment); gl.linkProgram(program);
	if (!gl.getProgramParameter(program, gl.LINK_STATUS)) { dispose(); return; }
	gl.useProgram(program);
	gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
	gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1,-1, 1,-1, -1,1, -1,1, 1,-1, 1,1]), gl.STATIC_DRAW);
	const position = gl.getAttribLocation(program, 'position');
	gl.enableVertexAttribArray(position); gl.vertexAttribPointer(position, 2, gl.FLOAT, false, 0, 0);
	gl.bindTexture(gl.TEXTURE_2D, texture);
	gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
	gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
	gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
	gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
	gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([30,20,40,255]));
	let ready = false, disposed = false;
	image.onload = () => {
		if (disposed) return;
		gl.bindTexture(gl.TEXTURE_2D, texture);
		gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, true);
		gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, image);
		ready = true;
	};
	image.src = '/companion/idle-head.png';
	const uniforms = Object.fromEntries(['time','wave','processing','yaw','pitch','blink','failure'].map(name => [name, gl.getUniformLocation(program, name)]));
	const resolution = gl.getUniformLocation(program, 'resolution');
	gl.uniform1i(gl.getUniformLocation(program, 'face'), 0);
	const ratio = Math.min(window.devicePixelRatio || 1, 1.5);
	canvas.width = Math.round(180 * ratio); canvas.height = Math.round(120 * ratio);
	gl.viewport(0, 0, canvas.width, canvas.height);
	return {
		draw(frame: CompanionFrame) {
			if (disposed || !ready || gl.isContextLost()) return false;
			for (const name of Object.keys(uniforms) as (keyof CompanionFrame)[]) gl.uniform1f(uniforms[name] ?? null, frame[name]);
			gl.uniform2f(resolution, canvas.width, canvas.height);
			gl.drawArrays(gl.TRIANGLES, 0, 6);
			return true;
		},
		dispose() { disposed = true; dispose(); },
	};
}
