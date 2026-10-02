import { writeFileSync, mkdirSync, readFileSync } from "node:fs";
import { fileURLToPath, URL } from "node:url";
import { Buffer } from "node:buffer";
import process from "node:process";
import console from "node:console";

/**
 * Original Jarvito vector artwork and animation rig. No marketplace artwork.
 * Minimal deterministic RIVE v7 writer using the MIT licensed runtime schema:
 * https://github.com/rive-app/rive-runtime/tree/0aadd4c65084a38dbeae3bd05814ead3f743ed77/include/rive/generated
 * Coordinates, poses, curves, choreography and character design are authored here.
 * Run: bun scripts/generate-jarvito-rive.mjs [--check]
 */
const chunks = [];
let remap;
const uint = (value) => {
  const bytes = [];
  do { bytes.push((value & 127) | (value > 127 ? 128 : 0)); value >>>= 7; } while (value);
  return Buffer.from(bytes);
};
const float = (value) => { const b = Buffer.alloc(4); b.writeFloatLE(value); return b; };
const color = (value) => { const b = Buffer.alloc(4); b.writeUInt32LE(value >>> 0); return b; };
const string = (value) => { const b = Buffer.from(value); return Buffer.concat([uint(b.length), b]); };
const stringKeys = new Set([4, 55, 138]);
const boolKeys = new Set([41, 50, 62, 141, 164, 196, 376]);
const uintKeys = new Set([5, 23, 40, 48, 49, 51, 53, 56, 57, 59, 60, 61, 67, 68, 69, 128, 129, 149, 151, 152, 155, 156, 158, 160, 165, 167, 236, 349, 350]);
const colorKeys = new Set([37, 38, 88]);
function object(type, props = {}) {
  chunks.push(uint(type));
  for (const [keyString, originalValue] of Object.entries(props)) {
    const key = Number(keyString);
    const value = remap && [5, 51, 69].includes(key) ? remap.get(originalValue) : originalValue;
    chunks.push(uint(key), stringKeys.has(key) ? string(value) : boolKeys.has(key) ? Buffer.from([Number(value)]) : uintKeys.has(key) ? uint(value) : colorKeys.has(key) ? color(value) : float(value));
  }
  chunks.push(uint(0));
}
chunks.push(Buffer.from("RIVE"), uint(7), uint(0), uint(0), uint(0));
object(23);
let nextId = 0;
const scene = [];
function component(type, name, parent, props = {}) {
  const id = nextId++;
  scene.push({ id, type, parent, props: { 4: name, ...(parent !== null ? { 5: parent } : {}), ...props } });
  return id;
}
component(1, "Jarvito", null, { 7: 120, 8: 120, 236: 0 });
function node(name, parent, x = 0, y = 0, props = {}) { return component(2, name, parent, { 13: x, 14: y, ...props }); }
function paint(shape, name, fill, stroke = false, thickness = 1, props = {}) {
  const id = component(stroke ? 24 : 20, `${name} paint`, shape, stroke ? { 47: thickness, 48: 1, 49: 1, ...props } : props);
  if (Array.isArray(fill)) {
    const gradient = component(22, `${name} gradient`, id, { 42: -35, 33: -35, 34: 25, 35: 40 });
    fill.forEach((c, i) => component(19, `${name} stop ${i}`, gradient, { 38: c, 39: i / (fill.length - 1) }));
  } else return component(18, `${name} color`, id, { 37: fill });
  return id;
}
const palette = { cyan: 0xff79e3f0, green: 0xffa2dd84, yellow: 0xffffce7b, red: 0xfff08892, white: 0xffebf8fa, outline: 0xff66798c, glass: [0xff23394b, 0xff101d2b, 0xff172634], shell: [0xffd4e2ee, 0xff849bb0, 0xff516b83] };
const lights = [];
function shape(name, parent, x, y, width, height, radius, fill, options = {}) {
  const id = component(3, name, parent, { 13: x, 14: y, ...(options.opacity !== undefined ? { 18: options.opacity } : {}) });
  component(options.ellipse ? 4 : 7, `${name} path`, id, { 20: width, 21: height, ...(!options.ellipse ? { 31: radius } : {}) });
  const fillId = paint(id, name, fill);
  if (options.light) lights.push(fillId);
  if (options.stroke) paint(id, `${name} rim`, options.stroke, true, options.strokeWidth ?? 1);
  return id;
}
function line(name, parent, points, stroke, thickness = 1, options = {}) {
  const id = component(3, name, parent, { ...(options.opacity !== undefined ? { 18: options.opacity } : {}) });
  const path = component(16, `${name} path`, id);
  const vertices = points.map((p, i) => component(6, `${name} point ${i}`, path, { 24: p[0], 25: p[1], 84: p[2] ?? 0, 85: p[3] ?? 0, 86: p[4] ?? 0, 87: p[5] ?? 0 }));
  const c = paint(id, name, stroke, true, thickness, options.fixedStroke ? { 50: false } : {});
  if (options.light) lights.push(c);
  return { id, vertices };
}
// A compact floating robot, with no legs or permanently exposed limbs.
// Its metal shell, visor and antenna are original Jarvito artwork.
shape("Shadow", 0, 60, 111, 54, 4, 0, 0x3210141c, { ellipse: true });
const rig = node("Rig", 0, 60, 70);
const breathing = node("Breathing", rig);
const dragPose = node("Drag pose", breathing);
const playfulBody = node("Playful body", dragPose);
const glide = node("Glide", playfulBody);
const leftArm = node("Left shoulder", glide, -39, 8);
const leftSwing = node("Left swing", leftArm);
const leftAppearance = node("Left hand reveal", leftSwing, 0, 0, { 18: 0 });
shape("Left upper arm", leftAppearance, -2, 8, 7, 18, 3.5, palette.shell, { stroke: palette.outline });
const leftForearm = node("Left wrist", leftAppearance, -2, 14);
shape("Left hand", leftForearm, 0, 2, 13, 16, 6, palette.shell, { stroke: palette.outline });
line("Left fingers", leftForearm, [[-2, 0], [-2, 5]], 0x7064798c, 1.2);
const head = node("Head", glide);
const headEmote = node("Head emote", head);
const attention = node("Attention", headEmote);
// Ears, soft beveled shell and cyan antenna retain the original Jarvito silhouette.
shape("Left earpiece", attention, -42, 0, 9, 23, 4.5, palette.shell, { stroke: palette.outline });
shape("Right earpiece", attention, 42, 0, 9, 23, 4.5, palette.shell, { stroke: palette.outline });
const antenna = node("Antenna", attention, 0, -37);
shape("Antenna stem", antenna, 0, -3, 4, 8, 2, palette.shell);
shape("Antenna halo", antenna, 0, -8, 13, 13, 0, palette.cyan, { ellipse: true, opacity: .1, light: true });
shape("Antenna lens", antenna, 0, -8, 7, 7, 0, palette.cyan, { ellipse: true, light: true });
shape("Antenna highlight", antenna, -1, -9, 2, 2, 0, palette.white, { ellipse: true, opacity: .9 });
shape("Head shell", attention, 0, 0, 84, 76, 27, palette.shell, { stroke: palette.outline, strokeWidth: 1.1 });
line("Shell upper bevel", attention, [[-23, -31], [23, -31]], 0x4aebf8fa, 1.5);
shape("Visor rim", attention, 0, 4, 73, 53, 19, 0xff192c3c, { stroke: 0xff5d9aab });
shape("Visor", attention, 0, 3, 68, 48, 17, palette.glass);
line("Visor reflection", attention, [[-22, -15], [19, -15]], 0x23b4ebf4, 1.2);
const blush = node("Blush", attention, 0, 0, { 18: .2 });
shape("Cheek left", blush, -25, 15, 9, 3, 1.5, palette.cyan, { light: true });
shape("Cheek right", blush, 25, 15, 9, 3, 1.5, palette.cyan, { light: true });
const gazeX = node("Gaze X", attention);
const gazeY = node("Gaze Y", gazeX);
const wander = node("Wander", gazeY);
const gestureGaze = node("Gesture gaze", wander);
const eyeExpression = node("Eye expression", gestureGaze);
const blink = node("Blink", eyeExpression, 0, -1);
const leftEye = node("Left eye", blink, -16, -2);
const rightEye = node("Right eye", blink, 16, -2);
const leftEmote = node("Left eye emote", leftEye);
const rightEmote = node("Right eye emote", rightEye);
const leftLens = node("Left eye open", leftEmote);
const rightLens = node("Right eye open", rightEmote);
shape("Left eye lens", leftLens, 0, 0, 12, 17, 6, palette.cyan, { light: true });
shape("Right eye lens", rightLens, 0, 0, 12, 17, 6, palette.cyan, { light: true });
shape("Left eye glint", leftLens, -1.5, -3, 2.2, 3.5, 1.1, palette.white, { opacity: .65 });
shape("Right eye glint", rightLens, -1.5, -3, 2.2, 3.5, 1.1, palette.white, { opacity: .65 });
const leftHappyEye = node("Left eye happy", leftEmote, 0, 0, { 16: 0, 18: 0 });
const rightHappyEye = node("Right eye happy", rightEmote, 0, 0, { 16: 0, 18: 0 });
line("Left happy eye curve", leftHappyEye, [[-5, 0, 0, 0, -.8, 6], [5, 0, Math.PI + .8, 6]], palette.cyan, 3.2, { light: true, fixedStroke: true });
line("Right happy eye curve", rightHappyEye, [[-5, 0, 0, 0, -.8, 6], [5, 0, Math.PI + .8, 6]], palette.cyan, 3.2, { light: true, fixedStroke: true });
const dizzyEyes = node("Dizzy eyes reveal", eyeExpression, 0, -3, { 18: 0 });
// Original continuous spiral geometry, independent of the native activity face.
const spiral = Array.from({ length: 21 }, (_, i) => {
  const t = i / 20;
  const angle = t * Math.PI * 3.5;
  const radius = .8 + 5.5 * t;
  const dx = 5.5 * Math.cos(angle) - radius * Math.sin(angle) * Math.PI * 3.5;
  const dy = 5.5 * Math.sin(angle) + radius * Math.cos(angle) * Math.PI * 3.5;
  const tangent = Math.atan2(dy, dx);
  const handle = Math.hypot(dx, dy) / 60;
  return [radius * Math.cos(angle), radius * Math.sin(angle), tangent + Math.PI, i ? handle : 0, tangent, i < 20 ? handle : 0];
});
for (const [name, x] of [["Left", -16], ["Right", 16]]) {
  const eye = node(`${name} spiral eye`, dizzyEyes, x);
  line(`${name} spiral`, eye, spiral, palette.cyan, 2.3, { light: true, fixedStroke: true });
}
const sleepingEyes = node("Sleeping eyes reveal", eyeExpression, 0, -2, { 16: 0, 18: 0 });
for (const [name, x] of [["Left", -16], ["Right", 16]]) {
  const eye = node(`${name} sleeping eye`, sleepingEyes, x);
  line(`${name} closed lid`, eye, [[-5, 0, 0, 0, .55, 4], [5, 0, Math.PI - .55, 4]], palette.cyan, 2.7, { light: true, fixedStroke: true });
}
// Shape-level transforms give every state a readable mouth silhouette at 96px.
// Keeping stroke width fixed preserves the thinking line and inverted sad curve.
const mouthEmote = node("Mouth emote", gestureGaze);
const mouth = node("Mouth expression", mouthEmote, 0, 12, { 17: .55 });
line("Smile", mouth, [[-8, 0, 0, 0, .75, 10], [8, 0, Math.PI - .75, 10]], palette.cyan, 2.8, { light: true, fixedStroke: true });
const openMouth = node("Open mouth reveal", gestureGaze, 0, 13, { 18: 0 });
shape("Surprised mouth", openMouth, 0, 0, 7, 8, 3.5, palette.cyan, { light: true });
const sleepingSmile = node("Sleeping smile reveal", gestureGaze, 0, 13, { 16: 0, 18: 0 });
line("Relaxed smile", sleepingSmile, [[-4, 0, 0, 0, .5, 4], [4, 0, Math.PI - .5, 4]], palette.cyan, 2.5, { light: true, fixedStroke: true });
const leftBrow = node("Left brow", gestureGaze, -16, -14, { 18: .32 });
const leftBrowEmote = node("Left brow emote", leftBrow);
shape("Left brow bar", leftBrowEmote, 0, 0, 14, 2.5, 1.25, palette.cyan, { light: true });
const rightBrow = node("Right brow", gestureGaze, 16, -14, { 18: .32 });
const rightBrowEmote = node("Right brow emote", rightBrow);
shape("Right brow bar", rightBrowEmote, 0, 0, 14, 2.5, 1.25, palette.cyan, { light: true });
shape("Chin light", attention, 0, 32, 12, 2, 1, palette.cyan, { opacity: .5, light: true });
shape("Shell screw", attention, 27, -27, 3.2, 3.2, 0, palette.cyan, { ellipse: true, opacity: .6, light: true });
const rightArm = node("Right shoulder", glide, 39, 8);
const rightSwing = node("Right swing", rightArm);
const greeting = node("Greeting", rightSwing);
const rightAppearance = node("Right hand reveal", greeting, 0, 0, { 18: 0 });
const greetingAppearance = node("Greeting hand reveal", greeting, 0, 0, { 18: 0 });
function rightHand(parent, name, direction = 1) {
  shape(`${name} arm`, parent, direction * 2, 8, 7, 18, 3.5, palette.shell, { stroke: palette.outline });
  const wrist = node(`${name} wrist`, parent, direction * 2, 14);
  shape(`${name} hand`, wrist, 0, 2, 13, 16, 6, palette.shell, { stroke: palette.outline });
  line(`${name} fingers`, wrist, [[direction * 2, 0], [direction * 2, 5]], 0x7064798c, 1.2);
  return wrist;
}
const rightForearm = rightHand(rightAppearance, "Right");
rightHand(greetingAppearance, "Greeting");
const leftStretch = node("Left stretch reveal", leftSwing, 0, 0, { 16: 0, 18: 0 });
const rightStretch = node("Right stretch reveal", rightSwing, 0, 0, { 16: 0, 18: 0 });
rightHand(leftStretch, "Left stretch", -1);
rightHand(rightStretch, "Right stretch");
const sleepParticles = node("Sleep particles", 0, 96, 29, { 16: 0, 18: 0 });
line("Small sleep Z", sleepParticles, [[-2, 0], [2, 0], [-2, 5], [2, 5]], palette.cyan, 1.5, { light: true, fixedStroke: true });
const distantSleepZ = node("Distant sleep Z", sleepParticles, 9, -10);
line("Large sleep Z", distantSleepZ, [[-3, 0], [3, 0], [-3, 6], [3, 6]], palette.cyan, 1.6, { light: true, fixedStroke: true });
const ease = component(28, "Soft ease", 0, { 63: .42, 64: 0, 65: .25, 66: 1 });
const settle = component(28, "Overshoot settle", 0, { 63: .2, 64: 1.18, 65: .45, 66: 1 });

// Rive draws siblings in reverse component order. Export a reversed preorder,
// keeping parents before children and remapping all animated object references.
const ordered = [];
function orderChildren(parent) {
  // ponytail: this small authored rig uses a scan; index parents if it grows to thousands.
  const children = scene.filter((item) => item.parent === parent);
  // Vertices define geometry, not draw order: reversing them discards the
  // authored outgoing/incoming handles and turns a two-point curve into a line.
  if (![16, 22].includes(scene[parent]?.type)) children.reverse();
  for (const child of children) {
    ordered.push(child);
    orderChildren(child.id);
  }
}
orderChildren(null);
remap = new Map(ordered.map((item, id) => [item.id, id]));
ordered.forEach((item) => object(item.type, item.props));

const animations = [];
function animation(name, duration, tracks = [], loop = true) {
  const id = animations.length;
  animations.push(name);
  object(31, { 55: name, 56: 60, 57: duration, 59: loop ? 1 : 0 });
  let lastObject = -1;
  for (const { id: objectId, key, frames, isColor = false, easing = ease } of tracks.sort((a, b) => a.id - b.id || a.key - b.key)) {
    if (objectId !== lastObject) { object(25, { 51: objectId }); lastObject = objectId; }
    object(26, { 53: key });
    for (const [frame, value] of frames) object(isColor ? 37 : 30, { 67: frame, 68: 2, 69: easing, [isColor ? 88 : 70]: value });
  }
  return id;
}
const track = (id, key, frames, options = {}) => ({ id, key, frames, ...options });
const fixed = (id, key, value) => track(id, key, [[0, value]]);
const neutral = [
  fixed(rig, 14, 70), fixed(rig, 15, 0), fixed(rig, 16, 1), fixed(rig, 17, 1),
  fixed(head, 15, 0), fixed(head, 13, 0), fixed(head, 14, 0), fixed(head, 16, 1), fixed(head, 17, 1),
  fixed(leftAppearance, 18, 0), fixed(rightAppearance, 18, 0), fixed(blush, 18, .2),
  fixed(leftArm, 15, 0), fixed(rightArm, 15, 0), fixed(leftForearm, 15, 0), fixed(rightForearm, 15, 0),
  fixed(leftEye, 17, 1), fixed(rightEye, 17, 1), fixed(leftEye, 15, 0), fixed(rightEye, 15, 0),
  fixed(leftLens, 16, 1), fixed(rightLens, 16, 1), fixed(leftLens, 18, 1), fixed(rightLens, 18, 1),
  fixed(leftHappyEye, 16, 0), fixed(rightHappyEye, 16, 0), fixed(leftHappyEye, 18, 0), fixed(rightHappyEye, 18, 0),
  fixed(eyeExpression, 13, 0), fixed(eyeExpression, 14, 0), fixed(leftBrow, 18, .32), fixed(rightBrow, 18, .32),
  fixed(leftBrow, 14, -14), fixed(rightBrow, 14, -14), fixed(leftBrow, 15, 0), fixed(rightBrow, 15, 0),
  fixed(mouth, 13, 0), fixed(mouth, 14, 12), fixed(mouth, 15, 0), fixed(mouth, 16, 1), fixed(mouth, 17, .55),
];
function pose(changes) {
  const merged = new Map(neutral.map((t) => [`${t.id}:${t.key}`, t]));
  changes.forEach((t) => merged.set(`${t.id}:${t.key}`, t));
  return [...merged.values()];
}
function withLight(tracks, c) { return [...tracks, ...lights.map((id) => track(id, 37, [[0, c]], { isColor: true }))]; }
const statusAnimations = [];
const statusStatic = [];
const idle = pose([
  track(head, 15, [[0, 0], [150, -.04], [300, 0], [470, .09], [530, .03], [620, 0], [860, -.06], [960, 0], [1200, .05], [1410, 0], [1800, 0]]),
  // Slow curious glances, one asymmetrical wink, then a brief sleepy exhale.
  track(leftEye, 17, [[0, 1], [670, 1], [695, .07], [710, .07], [727, 1], [1240, 1], [1300, .35], [1430, .35], [1500, 1], [1800, 1]]),
  track(rightEye, 17, [[0, 1], [1240, 1], [1300, .35], [1430, .35], [1500, 1], [1800, 1]]),
  track(mouth, 17, [[0, .55], [670, .55], [700, .85], [745, .55], [1280, .55], [1360, .1], [1440, .1], [1520, .55], [1800, .55]]),
  track(blush, 18, [[0, .2], [670, .2], [700, .52], [765, .2], [1800, .2]]),
]);
const thinking = pose([
  track(head, 15, [[0, 0], [50, -.13], [190, -.1], [260, .07], [390, .04], [470, -.13], [620, -.1], [720, 0]]),
  track(head, 14, [[0, 0], [50, -2], [200, -2], [290, 0], [440, -1], [640, -2], [720, 0]]),
  track(rightAppearance, 18, [[0, 0], [30, 0], [48, 1], [240, 1], [270, 0], [425, 0], [448, 1], [650, 1], [680, 0], [720, 0]]),
  track(rightArm, 15, [[0, 0], [55, 1.58], [220, 1.45], [285, .12], [385, .12], [460, 1.52], [630, 1.55], [720, 0]]),
  track(rightForearm, 15, [[0, 0], [55, 1.02], [220, .94], [285, 0], [385, 0], [460, .94], [630, 1.02], [720, 0]]),
  fixed(leftBrow, 18, 1), fixed(rightBrow, 18, 1),
  track(leftBrow, 14, [[0, -18], [240, -17], [400, -18], [720, -18]]),
  fixed(rightBrow, 14, -11.5),
  track(leftBrow, 15, [[0, -.22], [100, -.3], [340, -.13], [500, -.3], [720, -.22]]),
  track(rightBrow, 15, [[0, .17], [100, .09], [340, .22], [500, .09], [720, .17]]),
  fixed(leftEye, 17, .74), fixed(rightEye, 17, 1.04),
  fixed(eyeExpression, 13, -1.5), fixed(eyeExpression, 14, -2.2),
  fixed(mouth, 13, 2.5), fixed(mouth, 15, -.1), fixed(mouth, 16, .7), fixed(mouth, 17, .06),
]);
const waiting = pose([
  track(head, 15, [[0, 0], [45, .13], [150, .1], [210, -.055], [275, .02], [330, 0]]),
  track(leftArm, 15, [[0, 0], [60, -.75], [135, -.65], [205, 0], [330, 0]]),
  track(leftAppearance, 18, [[0, 0], [35, 0], [50, 1], [140, 1], [180, 0], [330, 0]]),
  track(head, 16, [[0, 1], [40, 1.025], [90, 1], [330, 1]]),
  track(head, 17, [[0, 1], [40, .97], [90, 1], [330, 1]]),
  fixed(leftBrow, 18, .9), fixed(rightBrow, 18, .9), fixed(leftBrow, 15, -.15), fixed(rightBrow, 15, .18),
  fixed(leftBrow, 14, -18), fixed(rightBrow, 14, -16),
  fixed(leftEye, 17, 1.18), fixed(rightEye, 17, 1.18),
  fixed(eyeExpression, 14, -1), fixed(mouth, 16, .8), fixed(mouth, 17, .75),
]);
const reconnecting = pose([
  track(head, 15, [[0, -.05], [70, -.12], [180, .12], [280, -.08], [360, -.05]]),
  fixed(leftBrow, 18, .9), fixed(rightBrow, 18, .9), fixed(leftEye, 17, .82), fixed(rightEye, 17, .82),
  fixed(mouth, 17, .04),
]);
const completed = pose([
  track(rig, 14, [[0, 70], [15, 73], [34, 60], [52, 70], [66, 68], [84, 70], [240, 70]], { easing: settle }),
  track(rig, 16, [[0, 1], [15, 1.1], [34, .9], [52, 1.06], [75, .99], [100, 1], [240, 1]], { easing: settle }),
  track(rig, 17, [[0, 1], [15, .9], [34, 1.1], [52, .96], [75, 1.01], [100, 1], [240, 1]], { easing: settle }),
  track(leftAppearance, 18, [[0, 0], [18, 1], [82, 1], [106, 0], [240, 0]]),
  track(rightAppearance, 18, [[0, 0], [18, 1], [82, 1], [106, 0], [240, 0]]),
  track(head, 15, [[0, 0], [35, -.12], [60, .09], [87, -.03], [115, 0], [180, 0]]),
  track(leftArm, 15, [[0, 0], [30, -1.6], [67, -.45], [105, -.08], [180, -.08]], { easing: settle }),
  track(rightArm, 15, [[0, 0], [30, 1.65], [67, .4], [105, .08], [180, .08]], { easing: settle }),
  fixed(leftLens, 16, 0), fixed(rightLens, 16, 0), fixed(leftLens, 18, 0), fixed(rightLens, 18, 0),
  fixed(leftHappyEye, 16, 1), fixed(rightHappyEye, 16, 1), fixed(leftHappyEye, 18, 1), fixed(rightHappyEye, 18, 1),
  fixed(leftBrow, 14, -17), fixed(rightBrow, 14, -17), fixed(leftBrow, 15, -.12), fixed(rightBrow, 15, .12),
  fixed(mouth, 16, 1.12), fixed(mouth, 17, 1.35),
  fixed(blush, 18, .65),
]);
const failed = pose([
  track(head, 15, [[0, 0], [22, -.09], [39, .09], [57, -.06], [78, .055], [115, .035], [190, .035]]),
  track(head, 14, [[0, 0], [60, 2], [120, 3], [190, 3]]),
  track(leftArm, 15, [[0, 0], [100, -.1], [190, -.1]]), track(rightArm, 15, [[0, 0], [100, .1], [190, .1]]),
  fixed(leftEye, 17, .58), fixed(rightEye, 17, .58), fixed(leftBrow, 18, 1), fixed(rightBrow, 18, 1),
  fixed(eyeExpression, 14, 1.5), fixed(leftBrow, 14, -12.5), fixed(rightBrow, 14, -12.5),
  fixed(leftBrow, 15, -.38), fixed(rightBrow, 15, .38), fixed(mouth, 14, 14), fixed(mouth, 17, -.95),
]);
[idle, thinking, waiting, reconnecting, completed, failed].forEach((tracks, i) => {
  const c = [palette.cyan, palette.cyan, palette.yellow, palette.yellow, palette.green, palette.red][i];
  statusAnimations.push(animation(["Idle · curious", "Thinking · hand on chin", "Waiting · listening", "Reconnecting · searching", "Complete · celebration", "Error · recovery"][i], [1800, 720, 330, 360, 240, 190][i], withLight(tracks, c), i < 4));
  statusStatic.push(animation(`Reduced motion ${i}`, 60, withLight(tracks.map((t) => ({ ...t, frames: [[0, t.frames.at(-1)[1]]] })), c), false));
});
const breath = animation("Soft breathing", 270, [track(breathing, 14, [[0, 0], [110, -1.7], [270, 0]]), track(breathing, 17, [[0, 1], [110, 1.017], [270, 1]])]);
const breathRest = animation("Breathing rest", 60, [fixed(breathing, 14, 0), fixed(breathing, 17, 1)], false);
const blinking = animation("Natural blink cadence", 540, [track(blink, 17, [[0, 1], [118, 1], [123, .06], [128, 1], [307, 1], [312, .07], [318, 1], [333, 1], [338, .09], [343, 1], [540, 1]])]);
const eyesRest = animation("Eyes rest", 60, [fixed(blink, 17, 1)], false);
const wandering = animation("Ambient eye wander", 720, [track(wander, 13, [[0, 0], [95, 0], [140, -1.3], [245, -1.3], [275, 0], [390, 0], [420, 1.6], [515, 1.6], [555, 0], [720, 0]]), track(wander, 14, [[0, 0], [145, -.5], [270, 0], [420, .7], [550, 0], [720, 0]])]);
const wanderRest = animation("Ambient rest", 60, [fixed(wander, 13, 0), fixed(wander, 14, 0)], false);
const dragged = animation("Lifted by user", 90, [track(dragPose, 15, [[0, 0], [18, -.11], [45, .08], [72, -.03], [90, -.035]]), track(dragPose, 14, [[0, 0], [30, -2], [90, -2]])], false);
const dragRest = animation("Put down", 60, [fixed(dragPose, 15, 0), fixed(dragPose, 14, 0)], false);
const hovered = animation("Warm greeting", 150, [track(greetingAppearance, 18, [[0, 0], [20, 1], [95, 1], [125, 0], [150, 0]]), track(greeting, 15, [[0, 0], [32, 2.25], [47, 2.5], [60, 2.05], [73, 2.5], [90, 2.2], [125, 0], [150, 0]]), track(antenna, 15, [[0, 0], [35, .06], [65, -.05], [100, 0], [150, 0]])], false);
const greetingRest = animation("Greeting rest", 60, [fixed(greeting, 15, 0), fixed(greetingAppearance, 18, 0), fixed(antenna, 15, 0)], false);
const attentive = animation("Island attention", 75, [track(attention, 17, [[0, 1], [30, 1.012], [75, 1.006]]), track(attention, 14, [[0, 0], [45, -.5], [75, -.3]])], false);
const attentionRest = animation("Attention rest", 60, [fixed(attention, 17, 1), fixed(attention, 14, 0)], false);
// Keep the old public input compatible: a legless character glides instead.
const walking = animation("Floating glide", 120, [track(glide, 15, [[0, -.04], [60, .04], [120, -.04]]), track(glide, 14, [[0, 0], [60, -1.5], [120, 0]])]);
const walkingRest = animation("Glide rest", 60, [fixed(glide, 15, 0), fixed(glide, 14, 0)], false);
const gazeXAnimations = [-1, 0, 1].map((v) => animation(`Gaze horizontal ${v}`, 60, [fixed(gazeX, 13, v * 3)], false));
const gazeYAnimations = [-1, 0, 1].map((v) => animation(`Gaze vertical ${v}`, 60, [fixed(gazeY, 14, v * 2)], false));
const emoteRest = [
  fixed(leftEmote, 16, 1), fixed(rightEmote, 16, 1), fixed(leftEmote, 17, 1), fixed(rightEmote, 17, 1), fixed(leftEmote, 18, 1), fixed(rightEmote, 18, 1),
  fixed(mouthEmote, 18, 1), fixed(mouthEmote, 17, 1), fixed(openMouth, 18, 0), fixed(dizzyEyes, 18, 0),
  fixed(playfulBody, 13, 0), fixed(playfulBody, 14, 0), fixed(playfulBody, 15, 0), fixed(playfulBody, 16, 1), fixed(playfulBody, 17, 1),
  fixed(headEmote, 15, 0), fixed(headEmote, 14, 0), fixed(gestureGaze, 13, 0), fixed(gestureGaze, 14, 0),
  fixed(leftBrowEmote, 14, 0), fixed(rightBrowEmote, 14, 0), fixed(leftBrowEmote, 15, 0), fixed(rightBrowEmote, 15, 0),
  fixed(sleepingEyes, 16, 0), fixed(sleepingEyes, 18, 0), fixed(sleepingSmile, 16, 0), fixed(sleepingSmile, 18, 0),
  fixed(sleepParticles, 16, 0), fixed(sleepParticles, 18, 0), fixed(sleepParticles, 14, 29), fixed(distantSleepZ, 14, -10),
  fixed(leftStretch, 16, 0), fixed(rightStretch, 16, 0), fixed(leftStretch, 18, 0), fixed(rightStretch, 18, 0), fixed(leftStretch, 15, 0), fixed(rightStretch, 15, 0),
];
const emotePose = (changes) => {
  const tracks = new Map(emoteRest.map(t => [`${t.id}:${t.key}`, t]));
  changes.forEach(t => tracks.set(`${t.id}:${t.key}`, t));
  return [...tracks.values()];
};
const emotes = [
  emoteRest,
  emotePose([track(leftEmote, 17, [[0, 1], [12, .06], [42, .06], [58, 1], [90, 1]]), track(mouthEmote, 17, [[0, 1], [18, 1.6], [52, 1.6], [75, 1], [90, 1]])]),
  emotePose([fixed(leftEmote, 17, 1.3), fixed(rightEmote, 17, 1.3), fixed(mouthEmote, 18, 0), fixed(openMouth, 18, 1)]),
  emotePose([fixed(leftEmote, 17, .1), fixed(rightEmote, 17, .1), fixed(mouthEmote, 17, .15)]),
  emotePose([
    track(leftEmote, 17, [[0, 1], [5, .05], [18, .05], [32, 1], [54, 1]]),
    track(rightEmote, 17, [[0, 1], [5, .05], [18, .05], [32, 1], [54, 1]]),
    track(playfulBody, 16, [[0, 1], [6, 1.13], [17, .96], [30, 1.025], [46, 1], [54, 1]], { easing: settle }),
    track(playfulBody, 17, [[0, 1], [6, .88], [17, 1.06], [30, .985], [46, 1], [54, 1]], { easing: settle }),
    track(playfulBody, 14, [[0, 0], [6, 2], [17, -1], [30, .4], [46, 0], [54, 0]]),
  ]),
  emotePose([
    fixed(leftEmote, 18, 0), fixed(rightEmote, 18, 0), fixed(dizzyEyes, 18, 1), fixed(mouthEmote, 17, .18),
    track(playfulBody, 15, [[0, 0], [8, -.1], [26, .28], [47, -.3], [66, .22], [92, -.18], [118, .14], [144, -.07], [180, 0]]),
    track(playfulBody, 13, [[0, 0], [26, 2.5], [47, -2.5], [66, 2], [92, -1.5], [118, 1], [144, -.5], [180, 0]]),
    track(playfulBody, 14, [[0, 0], [16, 1.8], [36, -1.3], [64, 1], [92, -1], [124, .5], [180, 0]]),
    track(playfulBody, 16, [[0, 1], [16, 1.06], [36, .97], [64, 1.03], [124, .99], [180, 1]]),
    track(playfulBody, 17, [[0, 1], [16, .94], [36, 1.04], [64, .98], [124, 1.01], [180, 1]]),
  ]),
  emotePose([
    // Closed vector eyelids stay readable at notch size; this is a sustained
    // resting pose, independent of the brief sleepy expression above.
    fixed(leftEmote, 16, 0), fixed(rightEmote, 16, 0), fixed(leftEmote, 18, 0), fixed(rightEmote, 18, 0), fixed(mouthEmote, 18, 0),
    fixed(sleepingEyes, 16, 1), fixed(sleepingEyes, 18, 1), fixed(sleepingSmile, 16, 1), fixed(sleepingSmile, 18, 1),
    track(headEmote, 15, [[0, .13], [120, .165], [240, .13]]),
    track(headEmote, 14, [[0, 1], [120, 1.5], [240, 1]]),
    track(playfulBody, 17, [[0, .985], [120, 1], [240, .985]]),
    fixed(leftBrowEmote, 14, 2), fixed(rightBrowEmote, 14, 2),
    fixed(sleepParticles, 16, 1),
    track(sleepParticles, 18, [[0, .6], [110, .85], [240, .6]]),
    track(sleepParticles, 14, [[0, 29], [120, 26], [240, 29]]),
    track(distantSleepZ, 14, [[0, -10], [120, -12], [240, -10]]),
  ]),
  emotePose([
    track(leftEmote, 17, [[0, 1], [16, .08], [40, .08], [64, 1], [120, 1]]),
    track(rightEmote, 17, [[0, 1], [16, .08], [40, .08], [64, 1], [120, 1]]),
    track(leftStretch, 16, [[0, 0], [10, 1], [80, 1], [110, 0], [120, 0]]),
    track(rightStretch, 16, [[0, 0], [10, 1], [80, 1], [110, 0], [120, 0]]),
    track(leftStretch, 18, [[0, 0], [12, 1], [80, 1], [110, 0], [120, 0]]),
    track(rightStretch, 18, [[0, 0], [12, 1], [80, 1], [110, 0], [120, 0]]),
    track(leftStretch, 15, [[0, 0], [30, 2.45], [55, 2.3], [80, 1.8], [110, 0], [120, 0]], { easing: settle }),
    track(rightStretch, 15, [[0, 0], [30, -2.45], [55, -2.3], [80, -1.8], [110, 0], [120, 0]], { easing: settle }),
    track(playfulBody, 16, [[0, 1], [18, 1.035], [38, .93], [62, .965], [86, 1.02], [110, 1], [120, 1]], { easing: settle }),
    track(playfulBody, 17, [[0, 1], [18, .965], [38, 1.055], [62, 1.025], [86, .99], [110, 1], [120, 1]], { easing: settle }),
    track(headEmote, 14, [[0, 0], [40, -1.5], [65, -1], [110, 0], [120, 0]]),
  ]),
  emotePose([
    track(headEmote, 15, [[0, 0], [25, -.2], [63, -.16], [88, .035], [112, 0], [120, 0]]),
    track(gestureGaze, 13, [[0, 0], [25, -2], [65, -2], [104, 0], [120, 0]]),
    track(gestureGaze, 14, [[0, 0], [25, -.7], [65, -.7], [104, 0], [120, 0]]),
    track(leftBrowEmote, 14, [[0, 0], [20, -3], [65, -3], [110, 0], [120, 0]]),
    track(rightBrowEmote, 14, [[0, 0], [20, 1], [65, 1], [110, 0], [120, 0]]),
    track(leftBrowEmote, 15, [[0, 0], [20, -.15], [65, -.15], [110, 0], [120, 0]]),
    track(rightBrowEmote, 15, [[0, 0], [20, .1], [65, .1], [110, 0], [120, 0]]),
    track(rightEmote, 17, [[0, 1], [30, .7], [65, .7], [108, 1], [120, 1]]),
    track(mouthEmote, 17, [[0, 1], [25, .7], [65, .7], [108, 1], [120, 1]]),
  ]),
];
const emoteAnimations = emotes.map((tracks, i) => animation(["Expression rest", "Expression wink", "Expression surprise", "Expression sleepy", "Expression poke", "Expression dizzy", "Expression sleeping", "Expression stretch", "Expression curious"][i], [90, 90, 90, 90, 90, 180, 240, 120, 120][i], tracks, i === 6));
const emoteStatic = emotes.map((tracks, i) => animation(`Reduced expression ${i}`, 60, tracks.map(t => ({ ...t, frames: [[0, i === 4 && [leftEmote, rightEmote].includes(t.id) && t.key === 17 ? .08 : t.frames.at(-1)[1]]] })), false));

object(53, { 55: "Jarvito" });
const inputNames = ["status", "hovered", "dragging", "reducedMotion", "lookX", "lookY", "expanded", "walking", "gesture"];
inputNames.forEach((name) => object(["status", "lookX", "lookY", "gesture"].includes(name) ? 56 : 59, { 138: name }));
const condition = (input, value, op = 0) => {
  const id = inputNames.indexOf(input);
  object(typeof value === "boolean" ? 71 : 70, { 155: id, 156: typeof value === "boolean" ? (value ? 0 : 1) : op, ...(typeof value !== "boolean" ? { 157: value } : {}) });
};
function transition(to, conditions = [], duration = 260) {
  object(65, { 151: to, 158: duration, 152: 32 });
  conditions.forEach(([input, value, op]) => condition(input, value, op));
}
function layer(name, states, routes, duration = 260) {
  object(57, { 138: name });
  object(63); transition(3, [], 0);
  object(62); routes.forEach(({ index, conditions, duration: routeDuration }) => transition(index + 3, conditions, routeDuration ?? duration));
  object(64);
  states.forEach((animationId) => object(61, { 149: animationId }));
}
layer("Emotions", [...statusAnimations, ...statusStatic], [
  ...statusAnimations.map((_, i) => ({ index: i, conditions: [["status", i], ["reducedMotion", false]] })),
  ...statusStatic.map((_, i) => ({ index: i + 6, conditions: [["status", i], ["reducedMotion", true]], duration: 0 })),
], 340);
function boolLayer(name, active, rest, input, extra = []) {
  layer(name, [rest, active], [
    { index: 0, conditions: [["reducedMotion", true]], duration: 0 },
    { index: 1, conditions: [[input, true], ["reducedMotion", false], ...extra] },
    { index: 0, conditions: [[input, false]] },
    ...extra.map(([key, value]) => ({ index: 0, conditions: [[key, !value]] })),
  ]);
}
layer("Breathing", [breath, breathRest], [{ index: 0, conditions: [["reducedMotion", false]] }, { index: 1, conditions: [["reducedMotion", true]], duration: 0 }]);
layer("Blink", [blinking, eyesRest], [{ index: 0, conditions: [["reducedMotion", false]] }, { index: 1, conditions: [["reducedMotion", true]], duration: 0 }], 90);
layer("Ambient gaze", [wandering, wanderRest], [{ index: 1, conditions: [["reducedMotion", true]], duration: 0 }, { index: 0, conditions: [["reducedMotion", false], ["hovered", false]] }, { index: 1, conditions: [["hovered", true]] }]);
boolLayer("Drag", dragged, dragRest, "dragging");
boolLayer("Greeting", hovered, greetingRest, "hovered", [["dragging", false]]);
boolLayer("Attention", attentive, attentionRest, "expanded");
boolLayer("Locomotion", walking, walkingRest, "walking", [["dragging", false]]);
function gazeLayer(name, input, ids) {
  object(57, { 138: name }); object(63); transition(3, [], 0); object(62); object(64);
  object(76, { 167: inputNames.indexOf(input) });
  ids.forEach((id, i) => object(75, { 165: id, 166: i - 1 }));
}
gazeLayer("Pointer X", "lookX", gazeXAnimations);
gazeLayer("Pointer Y", "lookY", gazeYAnimations);
layer("Playful expressions", [...emoteAnimations, ...emoteStatic], [
  ...emoteAnimations.map((_, i) => ({ index: i, conditions: [["gesture", i], ["reducedMotion", false]] })),
  ...emoteStatic.map((_, i) => ({ index: i + emoteAnimations.length, conditions: [["gesture", i], ["reducedMotion", true]], duration: 0 })),
], 180);

const output = Buffer.concat(chunks);
const path = fileURLToPath(new URL("../src/assets/companion/jarvito.riv", import.meta.url));
if (process.argv.includes("--check")) {
  if (!readFileSync(path).equals(output)) throw new Error("Jarvito .riv is stale; regenerate the original artwork.");
  console.log(`Jarvito .riv is reproducible (${output.length} bytes, ${animations.length} animations, ${nextId} vector components).`);
} else {
  mkdirSync(fileURLToPath(new URL("../src/assets/companion", import.meta.url)), { recursive: true });
  writeFileSync(path, output);
  console.log(`Generated original Jarvito .riv (${output.length} bytes, ${animations.length} animations, ${nextId} vector components).`);
}
