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
component(1, "Jarvito", null, { 7: 120, 8: 140, 236: 0 });
function node(name, parent, x = 0, y = 0, props = {}) { return component(2, name, parent, { 13: x, 14: y, ...props }); }
function paint(shape, name, fill, stroke = false, thickness = 1, props = {}) {
  const id = component(stroke ? 24 : 20, `${name} paint`, shape, stroke ? { 47: thickness, 48: 1, 49: 1, ...props } : props);
  if (Array.isArray(fill)) {
    const gradient = component(22, `${name} gradient`, id, { 42: -35, 33: -35, 34: 25, 35: 40 });
    fill.forEach((c, i) => component(19, `${name} stop ${i}`, gradient, { 38: c, 39: i / (fill.length - 1) }));
  } else return component(18, `${name} color`, id, { 37: fill });
  return id;
}
const palette = { cyan: 0xff56d9e9, green: 0xffa2dd84, yellow: 0xffffce7b, red: 0xfff08892, white: 0xffebf8fa, outline: 0xff414e5e, glass: [0xff1c2938, 0xff0e1825, 0xff111b2a], shell: [0xff667587, 0xff344150, 0xff202b39] };
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
// Grounding, articulated boots and torso; everything fits the existing 120x140 slot.
shape("Shadow", 0, 60, 132, 54, 5, 0, 0x4710141c, { ellipse: true });
const rig = node("Rig", 0, 60, 112);
const breathing = node("Breathing", rig);
const dragPose = node("Drag pose", breathing);
const leftLeg = node("Left leg", dragPose, -14, -7);
const leftStep = node("Left step", leftLeg);
shape("Left shin", leftStep, 0, 10, 12, 20, 5, palette.shell, { stroke: palette.outline });
shape("Left boot", leftStep, -1, 19, 19, 8, 4, palette.shell, { stroke: palette.outline });
shape("Left sole", leftStep, -1, 22, 15, 2, 1, palette.cyan, { opacity: .42, light: true });
const rightLeg = node("Right leg", dragPose, 14, -7);
const rightStep = node("Right step", rightLeg);
shape("Right shin", rightStep, 0, 10, 12, 20, 5, palette.shell, { stroke: palette.outline });
shape("Right boot", rightStep, 1, 19, 19, 8, 4, palette.shell, { stroke: palette.outline });
shape("Right sole", rightStep, 1, 22, 15, 2, 1, palette.cyan, { opacity: .42, light: true });
shape("Torso", dragPose, 0, -11, 45, 27, 11, palette.shell, { stroke: palette.outline });
shape("Chest inset", dragPose, 0, -7, 24, 7, 3, 0xff172330);
shape("Chest status", dragPose, 0, -7, 13, 2.5, 1.25, palette.cyan, { light: true });
const leftArm = node("Left shoulder", dragPose, -39, -33);
const leftSwing = node("Left swing", leftArm);
shape("Left shoulder joint", leftSwing, 0, 2, 11, 11, 0, 0xff203042, { ellipse: true, stroke: palette.outline });
shape("Left upper arm", leftSwing, -2, 10, 13, 23, 6, palette.shell, { stroke: palette.outline });
const leftForearm = node("Left wrist", leftSwing, -2, 19);
shape("Left hand", leftForearm, 0, 2, 14, 15, 6, palette.shell, { stroke: palette.outline });
shape("Left hand glint", leftForearm, -2, 2, 2, 6, 1, palette.cyan, { opacity: .65, light: true });
const head = node("Head", dragPose, 0, -44);
const attention = node("Attention", head);
// Ears, soft beveled shell and cyan antenna retain the original Jarvito silhouette.
shape("Left earpiece", attention, -41, 0, 12, 28, 6, palette.shell, { stroke: palette.outline });
shape("Right earpiece", attention, 41, 0, 12, 28, 6, palette.shell, { stroke: palette.outline });
const antenna = node("Antenna", attention, 0, -37);
shape("Antenna stem", antenna, 0, -8, 4.5, 16, 2.25, palette.shell);
shape("Antenna halo", antenna, 0, -18, 15, 15, 0, palette.cyan, { ellipse: true, opacity: .12, light: true });
shape("Antenna lens", antenna, 0, -18, 9, 9, 0, palette.cyan, { ellipse: true, light: true });
shape("Antenna highlight", antenna, -1.3, -19.5, 3, 3, 0, palette.white, { ellipse: true, opacity: .85 });
shape("Head shell", attention, 0, 0, 83, 77, 24, palette.shell, { stroke: palette.outline, strokeWidth: 1.25 });
line("Shell upper bevel", attention, [[-23, -31], [23, -31]], 0x4aebf8fa, 1.5);
shape("Visor rim", attention, 0, 4, 72, 50, 17, 0xff192c3c, { stroke: 0xff407482 });
shape("Visor", attention, 0, 3, 66, 44, 14, palette.glass);
line("Visor reflection", attention, [[-22, -15], [19, -15]], 0x23b4ebf4, 1.2);
shape("Cheek left", attention, -25, 15, 7, 2, 1, palette.cyan, { opacity: .15, light: true });
shape("Cheek right", attention, 25, 15, 7, 2, 1, palette.cyan, { opacity: .15, light: true });
const gazeX = node("Gaze X", attention);
const gazeY = node("Gaze Y", gazeX);
const wander = node("Wander", gazeY);
const eyeExpression = node("Eye expression", wander);
const blink = node("Blink", eyeExpression, 0, -1);
const leftEye = node("Left eye", blink, -16, -2);
const rightEye = node("Right eye", blink, 16, -2);
const leftLens = node("Left eye open", leftEye);
const rightLens = node("Right eye open", rightEye);
shape("Left eye lens", leftLens, 0, 0, 10, 14, 5, palette.cyan, { light: true });
shape("Right eye lens", rightLens, 0, 0, 10, 14, 5, palette.cyan, { light: true });
shape("Left eye glint", leftLens, -1.5, -3, 2.2, 3.5, 1.1, palette.white, { opacity: .65 });
shape("Right eye glint", rightLens, -1.5, -3, 2.2, 3.5, 1.1, palette.white, { opacity: .65 });
const leftHappyEye = node("Left eye happy", leftEye, 0, 0, { 16: 0, 18: 0 });
const rightHappyEye = node("Right eye happy", rightEye, 0, 0, { 16: 0, 18: 0 });
line("Left happy eye curve", leftHappyEye, [[-5, 0, 0, 0, -.8, 6], [5, 0, Math.PI + .8, 6]], palette.cyan, 3.2, { light: true, fixedStroke: true });
line("Right happy eye curve", rightHappyEye, [[-5, 0, 0, 0, -.8, 6], [5, 0, Math.PI + .8, 6]], palette.cyan, 3.2, { light: true, fixedStroke: true });
// Shape-level transforms give every state a readable mouth silhouette at 96px.
// Keeping stroke width fixed preserves the thinking line and inverted sad curve.
const mouth = node("Mouth expression", wander, 0, 12, { 17: .55 });
line("Smile", mouth, [[-8, 0, 0, 0, .75, 10], [8, 0, Math.PI - .75, 10]], palette.cyan, 2.8, { light: true, fixedStroke: true });
const leftBrow = node("Left brow", wander, -16, -14, { 18: .32 });
shape("Left brow bar", leftBrow, 0, 0, 14, 2.5, 1.25, palette.cyan, { light: true });
const rightBrow = node("Right brow", wander, 16, -14, { 18: .32 });
shape("Right brow bar", rightBrow, 0, 0, 14, 2.5, 1.25, palette.cyan, { light: true });
shape("Chin light", attention, 0, 32, 12, 2, 1, palette.cyan, { opacity: .5, light: true });
shape("Shell screw", attention, 27, -27, 3.2, 3.2, 0, palette.cyan, { ellipse: true, opacity: .6, light: true });
const rightArm = node("Right shoulder", dragPose, 39, -33);
const rightSwing = node("Right swing", rightArm);
const greeting = node("Greeting", rightSwing);
shape("Right shoulder joint", greeting, 0, 2, 11, 11, 0, 0xff203042, { ellipse: true, stroke: palette.outline });
shape("Right upper arm", greeting, 2, 10, 13, 23, 6, palette.shell, { stroke: palette.outline });
const rightForearm = node("Right wrist", greeting, 2, 19);
shape("Right hand", rightForearm, 0, 2, 14, 15, 6, palette.shell, { stroke: palette.outline });
shape("Right hand glint", rightForearm, 2, 2, 2, 6, 1, palette.cyan, { opacity: .65, light: true });
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
const neutral = [fixed(rig, 14, 112), fixed(rig, 15, 0), fixed(head, 15, 0), fixed(head, 13, 0), fixed(head, 14, -44), fixed(leftArm, 15, 0), fixed(rightArm, 15, 0), fixed(leftForearm, 15, 0), fixed(rightForearm, 15, 0), fixed(leftEye, 17, 1), fixed(rightEye, 17, 1), fixed(leftEye, 15, 0), fixed(rightEye, 15, 0), fixed(leftLens, 16, 1), fixed(rightLens, 16, 1), fixed(leftLens, 18, 1), fixed(rightLens, 18, 1), fixed(leftHappyEye, 16, 0), fixed(rightHappyEye, 16, 0), fixed(leftHappyEye, 18, 0), fixed(rightHappyEye, 18, 0), fixed(eyeExpression, 13, 0), fixed(eyeExpression, 14, 0), fixed(leftBrow, 18, .32), fixed(rightBrow, 18, .32), fixed(leftBrow, 14, -14), fixed(rightBrow, 14, -14), fixed(leftBrow, 15, 0), fixed(rightBrow, 15, 0), fixed(mouth, 13, 0), fixed(mouth, 14, 12), fixed(mouth, 15, 0), fixed(mouth, 16, 1), fixed(mouth, 17, .55)];
function pose(changes) {
  const merged = new Map(neutral.map((t) => [`${t.id}:${t.key}`, t]));
  changes.forEach((t) => merged.set(`${t.id}:${t.key}`, t));
  return [...merged.values()];
}
function withLight(tracks, c) { return [...tracks, ...lights.map((id) => track(id, 37, [[0, c]], { isColor: true }))]; }
const statusAnimations = [];
const statusStatic = [];
const idle = pose([
  track(head, 15, [[0, 0], [150, -.028], [300, 0], [390, .065], [440, .025], [510, 0], [650, -.04], [760, 0], [900, 0]]),
  track(leftArm, 15, [[0, .025], [180, -.035], [360, .025], [570, -.025], [900, .025]]),
  track(rightArm, 15, [[0, -.025], [180, .025], [360, -.025], [610, .018], [900, -.025]]),
]);
const thinking = pose([
  track(head, 15, [[0, 0], [50, -.13], [190, -.1], [260, .07], [390, .04], [470, -.13], [620, -.1], [720, 0]]),
  track(head, 14, [[0, -44], [50, -46], [200, -46], [290, -44], [440, -45], [640, -46], [720, -44]]),
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
  track(leftArm, 15, [[0, 0], [60, -.18], [180, -.12], [270, 0], [330, 0]]),
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
  track(rig, 14, [[0, 112], [15, 114], [34, 101], [52, 112], [66, 110], [84, 112], [180, 112]], { easing: settle }),
  track(head, 15, [[0, 0], [35, -.12], [60, .09], [87, -.03], [115, 0], [180, 0]]),
  track(leftArm, 15, [[0, 0], [30, -1.6], [67, -.45], [105, -.08], [180, -.08]], { easing: settle }),
  track(rightArm, 15, [[0, 0], [30, 1.65], [67, .4], [105, .08], [180, .08]], { easing: settle }),
  fixed(leftLens, 16, 0), fixed(rightLens, 16, 0), fixed(leftLens, 18, 0), fixed(rightLens, 18, 0),
  fixed(leftHappyEye, 16, 1), fixed(rightHappyEye, 16, 1), fixed(leftHappyEye, 18, 1), fixed(rightHappyEye, 18, 1),
  fixed(leftBrow, 14, -17), fixed(rightBrow, 14, -17), fixed(leftBrow, 15, -.12), fixed(rightBrow, 15, .12),
  fixed(mouth, 16, 1.12), fixed(mouth, 17, 1.35),
]);
const failed = pose([
  track(head, 15, [[0, 0], [22, -.09], [39, .09], [57, -.06], [78, .055], [115, .035], [190, .035]]),
  track(head, 14, [[0, -44], [60, -42], [120, -41], [190, -41]]),
  track(leftArm, 15, [[0, 0], [100, -.1], [190, -.1]]), track(rightArm, 15, [[0, 0], [100, .1], [190, .1]]),
  fixed(leftEye, 17, .58), fixed(rightEye, 17, .58), fixed(leftBrow, 18, 1), fixed(rightBrow, 18, 1),
  fixed(eyeExpression, 14, 1.5), fixed(leftBrow, 14, -12.5), fixed(rightBrow, 14, -12.5),
  fixed(leftBrow, 15, -.38), fixed(rightBrow, 15, .38), fixed(mouth, 14, 14), fixed(mouth, 17, -.95),
]);
[idle, thinking, waiting, reconnecting, completed, failed].forEach((tracks, i) => {
  const c = [palette.cyan, palette.cyan, palette.yellow, palette.yellow, palette.green, palette.red][i];
  statusAnimations.push(animation(["Idle · curious", "Thinking · hand on chin", "Waiting · listening", "Reconnecting · searching", "Complete · celebration", "Error · recovery"][i], [900, 720, 330, 360, 180, 190][i], withLight(tracks, c), i < 4));
  statusStatic.push(animation(`Reduced motion ${i}`, 60, withLight(tracks.map((t) => ({ ...t, frames: [[0, t.frames.at(-1)[1]]] })), c), false));
});
const breath = animation("Soft breathing", 270, [track(breathing, 14, [[0, 0], [110, -1.35], [270, 0]]), track(breathing, 17, [[0, 1], [110, 1.009], [270, 1]])]);
const breathRest = animation("Breathing rest", 60, [fixed(breathing, 14, 0), fixed(breathing, 17, 1)], false);
const blinking = animation("Natural blink cadence", 540, [track(blink, 17, [[0, 1], [118, 1], [123, .06], [128, 1], [307, 1], [312, .07], [318, 1], [333, 1], [338, .09], [343, 1], [540, 1]])]);
const eyesRest = animation("Eyes rest", 60, [fixed(blink, 17, 1)], false);
const wandering = animation("Ambient eye wander", 720, [track(wander, 13, [[0, 0], [95, 0], [140, -1.3], [245, -1.3], [275, 0], [390, 0], [420, 1.6], [515, 1.6], [555, 0], [720, 0]]), track(wander, 14, [[0, 0], [145, -.5], [270, 0], [420, .7], [550, 0], [720, 0]])]);
const wanderRest = animation("Ambient rest", 60, [fixed(wander, 13, 0), fixed(wander, 14, 0)], false);
const dragged = animation("Lifted by user", 90, [track(dragPose, 15, [[0, 0], [18, -.11], [45, .08], [72, -.03], [90, -.035]]), track(dragPose, 14, [[0, 0], [30, -2], [90, -2]])], false);
const dragRest = animation("Put down", 60, [fixed(dragPose, 15, 0), fixed(dragPose, 14, 0)], false);
const hovered = animation("Warm greeting", 120, [track(greeting, 15, [[0, 0], [32, 2.25], [47, 2.5], [60, 2.05], [73, 2.5], [90, 2.2], [120, .06]]), track(antenna, 15, [[0, 0], [35, .06], [65, -.05], [100, 0], [120, 0]])], false);
const greetingRest = animation("Greeting rest", 60, [fixed(greeting, 15, 0), fixed(antenna, 15, 0)], false);
const attentive = animation("Island attention", 75, [track(attention, 17, [[0, 1], [30, 1.012], [75, 1.006]]), track(attention, 14, [[0, 0], [45, -.5], [75, -.3]])], false);
const attentionRest = animation("Attention rest", 60, [fixed(attention, 17, 1), fixed(attention, 14, 0)], false);
const walking = animation("Walk cycle", 60, [track(leftStep, 15, [[0, -.16], [15, 0], [30, .16], [45, 0], [60, -.16]]), track(rightStep, 15, [[0, .16], [15, 0], [30, -.16], [45, 0], [60, .16]]), track(leftStep, 14, [[0, 0], [15, -2], [30, 0], [45, 0], [60, 0]]), track(rightStep, 14, [[0, 0], [15, 0], [30, 0], [45, -2], [60, 0]]), track(leftSwing, 15, [[0, .09], [30, -.09], [60, .09]]), track(rightSwing, 15, [[0, -.09], [30, .09], [60, -.09]])]);
const walkingRest = animation("Walking rest", 60, [fixed(leftStep, 15, 0), fixed(rightStep, 15, 0), fixed(leftStep, 14, 0), fixed(rightStep, 14, 0), fixed(leftSwing, 15, 0), fixed(rightSwing, 15, 0)], false);
const gazeXAnimations = [-1, 0, 1].map((v) => animation(`Gaze horizontal ${v}`, 60, [fixed(gazeX, 13, v * 3)], false));
const gazeYAnimations = [-1, 0, 1].map((v) => animation(`Gaze vertical ${v}`, 60, [fixed(gazeY, 14, v * 2)], false));

object(53, { 55: "Jarvito" });
const inputNames = ["status", "hovered", "dragging", "reducedMotion", "lookX", "lookY", "expanded", "walking"];
inputNames.forEach((name, i) => object([0, 4, 5].includes(i) ? 56 : 59, { 138: name }));
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
