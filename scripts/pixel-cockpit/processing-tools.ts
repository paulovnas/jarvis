import { invoke } from "@tauri-apps/api/core";

type RecordValue = Record<string, unknown>;
const record = (value: unknown): RecordValue => value && typeof value === "object" ? value as RecordValue : {};
const records = (value: unknown): RecordValue[] => Array.isArray(value) ? value.map(record) : [];

export interface ProcessedImage {
  kind: string;
  name: string;
  dataUrl: string;
  width: number;
  height: number;
}
export interface ProcessingResult { images: ProcessedImage[]; report: RecordValue }
export interface ProcessingSource { id: string; name: string; dataUrl: string; prompt?: string; seed?: string; frameId?: string }

export function processingSelection(state: RecordValue) {
  return JSON.stringify([record(state.selected).id, state.selectedFrameId, state.activeActionName]);
}

export function processedActionGeometry(value: unknown) {
  const action=record(value);
  const cell=record(action.cell), anchor=record(action.anchor);
  if (!Array.isArray(action.frameIds) || !action.frameIds.some((id: unknown) => typeof id === "string" && id.startsWith("processed_frame_"))) return;
  const width=Number(cell.width), height=Number(cell.height), x=Number(anchor.x), y=Number(anchor.y);
  if (![width,height,x,y].every(Number.isInteger) || width<1 || height<1 || width*height>24*1024*1024 || x<0 || y<0 || x>=width || y>=height) return;
  return {cell:{width,height},anchor:{x,y}};
}

export function processedAnimation(value: unknown) {
  const metadata=record(record(value).pixelProcessing);
  const frames=Number(metadata.frames), columns=Number(metadata.columns), rows=Number(metadata.rows);
  if (![frames,columns,rows].every(Number.isInteger) || frames<1 || frames>64 || columns<1 || rows<1 || columns*rows<frames || columns*rows>64) return;
  return {frames,grid:{columns,rows,gutter:0}};
}

export function processedAnimationPack<T extends {frames:{sourceFrameIndex:number;sheetRect:{x:number;y:number;width:number;height:number}}[];cell:{width:number;height:number};pivots:{x:number;y:number}[];anchors:{x:number;y:number}[]}>(value:unknown,pack:T):T {
  const processed=processedAnimation(value);
  if (!processed) return pack;
  const anchor=record(record(record(value).pixelProcessing).anchor);
  const x=Number(anchor.x),y=Number(anchor.y);
  const normalized=Number.isFinite(x)&&Number.isFinite(y)&&x>=0&&y>=0&&x<pack.cell.width&&y<pack.cell.height ? {x:x/pack.cell.width,y:y/pack.cell.height} : undefined;
  return {...pack,frames:pack.frames.map((frame) => ({...frame,sheetRect:{...frame.sheetRect,x:(frame.sourceFrameIndex%processed.grid.columns)*pack.cell.width,y:Math.floor(frame.sourceFrameIndex/processed.grid.columns)*pack.cell.height}})),...(normalized?{pivots:pack.pivots.map(point=>({...point,...normalized})),anchors:pack.anchors.map(point=>({...point,...normalized}))}:{})};
}

export function selectProcessingSource(state: RecordValue, args: RecordValue): ProcessingSource {
  if (args.sourceId && args.frameId) throw new Error("Informe apenas sourceId ou frameId.");
  const frame = args.frameId ? records(state.frames).find((item) => item.id === args.frameId) : undefined;
  if (args.frameId && !frame) throw new Error("Quadro não encontrado na linha do tempo.");
  const source = args.sourceId ? records(state.history).find((item) => item.id === args.sourceId)
    : frame ? records(state.history).find((item) => item.id === frame.sourceId) ?? frame : record(state.selected);
  if (!source || typeof source.id !== "string" || typeof (frame?.dataUrl ?? source.dataUrl) !== "string") {
    throw new Error("Imagem não encontrada. Selecione uma referência do histórico ou informe frameId.");
  }
  return {
    id: source.id, name: String(frame?.name ?? source.name ?? "imagem.png"),
    dataUrl: String(frame?.dataUrl ?? source.dataUrl), frameId: frame ? String(frame.id) : undefined,
    prompt: typeof source.prompt === "string" ? source.prompt : "", seed: typeof source.seed === "string" ? source.seed : "",
  };
}

/** Raster decoding belongs to the display boundary; grid detection and component extraction run in Rust. */
async function pngPixels(source: string): Promise<string> {
  if (source.startsWith("data:image/png;base64,")) return source;
  if (source.startsWith("data:image/gif") || /\.gif(?:$|[?#])/i.test(source)) throw new Error("Use a spritesheet PNG ou um quadro estático; um GIF não será achatado silenciosamente.");
  if (!source.startsWith("data:image/") && new URL(source, location.href).origin !== location.origin) throw new Error("A imagem deve pertencer a esta sessão.");
  const image = new Image();
  image.src = source;
  await image.decode();
  if (image.width * image.height > 8 * 1024 * 1024) throw new Error("A imagem excede 8 megapixels.");
  const canvas = document.createElement("canvas");
  canvas.width = image.width; canvas.height = image.height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("Não foi possível ler os pixels da imagem.");
  context.drawImage(image, 0, 0);
  return canvas.toDataURL("image/png");
}

export async function processNativeImage(operation: "recover_grid" | "extract_poses", source: string, options: RecordValue) {
  return invoke<ProcessingResult>("plugin:pixel-processing|process_pixel_art_image", { request: { operation, image: await pngPixels(source), options } });
}

export function deriveProcessedAssets(state: RecordValue, source: ProcessingSource, result: ProcessingResult, receipt: string, addToTimeline: boolean) {
  const now = new Date().toISOString();
  const base = source.name.replace(/\.[^.]+$/, "");
  const history = result.images.map((image, index) => ({
    id: "processed_" + receipt + "_" + index, name: base + "-" + image.name, dataUrl: image.dataUrl,
    size: image.width + "x" + image.height, provider: "local-file", source: "import", adopted: false,
    prompt: source.prompt ?? "", seed: source.seed ?? "", createdAt: now,
    derivedFromId: source.id, derivedFromName: source.name,
  }));
  const sourceId = history.find((_, index) => result.images[index].kind === "sheet")?.id ?? history[0]?.id;
  const frameImages = result.images.filter((image) => image.kind === "frame");
  const images = frameImages.length ? frameImages : result.images.filter((image) => image.kind === "native");
  const frames = addToTimeline ? images.map((image, index) => ({
    id: "processed_frame_" + receipt + "_" + index, name: base + " · " + (index + 1), sourceId,
    dataUrl: image.dataUrl, width: image.width, height: image.height, index,
  })) : [];
  const active = record(state.activeAction);
  const existing = new Set(records(state.actions).map((item) => item.name));
  let name = "Poses extraídas";
  for (let suffix = 2; existing.has(name); suffix++) name = "Poses extraídas " + suffix;
  const width = Number(result.report.frameWidth ?? images[0]?.width ?? 1);
  const height = Number(result.report.frameHeight ?? images[0]?.height ?? 1);
  const anchor = record(result.report.anchor);
  const action = frames.length ? {
    name, fps: typeof active.fps === "number" ? active.fps : 12,
    loop: typeof active.loop === "boolean" ? active.loop : true,
    playbackMode: active.playbackMode === "ping-pong-reverse" ? "ping-pong-reverse" : "normal",
    frameIds: frames.map((frame) => frame.id), cell: { width, height },
    anchor: { x: Number(anchor.x ?? Math.floor(width / 2)), y: Number(anchor.y ?? height - 1) },
  } : undefined;
  if (action) {
    const target=history.find((item) => item.id===sourceId);
    if (target) Object.assign(target,{pixelProcessing:{frames:frames.length,columns:Number(result.report.columns ?? 1),rows:Number(result.report.rows ?? 1),anchor:action.anchor}});
  }
  return { history, frames, action };
}
