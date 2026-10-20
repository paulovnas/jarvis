import { describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { deriveProcessedAssets, selectProcessingSource, processedActionGeometry, processingSelection, processedAnimation, processedAnimationPack, processNativeImage } from "./processing-tools";
import { createAnimationPackV2, validateAnimationPackV2 } from "../../src-tauri/resources/pixel-cockpit/upstream/src/lib/animationPackV2";
vi.mock("@tauri-apps/api/core",()=>({invoke:vi.fn()}));

describe("shared pixel processing operations", () => {
  const original = { id:"source",name:"hero.png",dataUrl:"data:image/png;base64,original",size:"40x20",prompt:"approved",seed:"12",adopted:true,animationQuality:{identityScore:100} };
  const state = { selected:original,history:[original],frames:[{id:"old-frame",sourceId:"source",dataUrl:original.dataUrl,width:40,height:20}],actions:[{name:"walk",fps:7,loop:true,frameIds:["old-frame"],cell:{width:40,height:20},anchor:{x:20,y:19}}],activeAction:{name:"walk",fps:7,loop:true,playbackMode:"normal"} };
  const result = { images:[{kind:"sheet",name:"poses-extraidas.png",dataUrl:"data:image/png;base64,sheet",width:48,height:24},{kind:"frame",name:"pose-01.png",dataUrl:"data:image/png;base64,frame",width:24,height:24}],report:{columns:2,rows:1,frameWidth:24,frameHeight:24,anchor:{x:12,y:21},warnings:[]} };

  it("uses only the isolated native plugin command and rejects flattening animated input",async()=>{
    vi.mocked(invoke).mockResolvedValueOnce(result);
    await expect(processNativeImage("recover_grid",original.dataUrl,{})).resolves.toBe(result);
    expect(invoke).toHaveBeenCalledWith("plugin:pixel-processing|process_pixel_art_image",{request:{operation:"recover_grid",image:original.dataUrl,options:{}}});
    await expect(processNativeImage("recover_grid","data:image/gif;base64,anything",{})).rejects.toThrow("GIF");
  });

  it("adds derived history and a separate action without rewriting accepted art or timing", () => {
    const before=JSON.stringify(state);
    const change=deriveProcessedAssets(state,original,result,"receipt-1",true);
    expect(change.history).toHaveLength(2);
    expect(change.history[0]).toMatchObject({derivedFromId:"source",adopted:false});
    expect(change.history[0]).not.toHaveProperty("animationQuality");
    expect(change.action).toMatchObject({fps:7,loop:true,playbackMode:"normal",anchor:{x:12,y:21}});
    expect(change.action?.frameIds).toEqual(change.frames.map(f=>f.id));
    expect(change.frames[0].sourceId).toBe(change.history[0].id);
    expect(processedAnimation(change.history[0])).toEqual({frames:1,grid:{columns:2,rows:1,gutter:0}});
    expect(processedAnimation(original)).toBeUndefined();
    expect(JSON.stringify(state)).toBe(before);
  });

  it("selects only actual history or timeline images and rejects fabricated source ids", () => {
    expect(selectProcessingSource(state,{}).id).toBe("source");
    expect(selectProcessingSource(state,{frameId:"old-frame"})).toMatchObject({id:"source",frameId:"old-frame"});
    expect(selectProcessingSource({...state,history:[]},{frameId:"old-frame"})).toMatchObject({id:"old-frame",frameId:"old-frame",dataUrl:original.dataUrl});
    expect(()=>selectProcessingSource(state,{sourceId:"invented"})).toThrow("Imagem");
    expect(()=>selectProcessingSource(state,{frameId:"invented"})).toThrow("Quadro");
    expect(()=>selectProcessingSource(state,{sourceId:"source",frameId:"old-frame"})).toThrow("apenas");
  });

  it("keeps grid recovery out of the timeline unless explicitly requested", () => {
    const change=deriveProcessedAssets(state,original,result,"receipt-2",false);
    expect(change.frames).toEqual([]);
    expect(change.action).toBeUndefined();
    expect(change.history[0].seed).toBe("12");
  });

  it("keeps native action geometry and timing when the suite hydrates it", () => {
    const action=deriveProcessedAssets(state,original,result,"receipt-1",true).action!;
    expect(processedActionGeometry(action)).toEqual({cell:{width:24,height:24},anchor:{x:12,y:21}});
    expect(processedActionGeometry(state.actions[0])).toBeUndefined();
    expect(processedActionGeometry({...action,cell:{width:-1,height:24}})).toBeUndefined();
    expect(processedActionGeometry({...action,cell:{width:8196,height:8},anchor:{x:4096,y:5}})).toEqual({cell:{width:8196,height:8},anchor:{x:4096,y:5}});
    expect(action.fps).toBe(7);
  });

  it("detects a newer action or frame selection even for the same source image", () => {
    const before={...state,selectedFrameId:"old-frame",activeActionName:"walk"};
    expect(processingSelection(before)).toBe(processingSelection({...before}));
    expect(processingSelection(before)).not.toBe(processingSelection({...before,selectedFrameId:"another-frame"}));
    expect(processingSelection(before)).not.toBe(processingSelection({...before,activeActionName:"idle"}));
  });

  it("exports all nine poses in the actual atlas grid instead of truncating to a recipe frame budget", () => {
    const source={pixelProcessing:{frames:9,columns:5,rows:2,anchor:{x:12,y:25}}};
    const pack=createAnimationPackV2({title:"Poses",actionId:"walk",directions:["poses"],framesPerDirection:9,defaultFps:7,loopMode:"loop",grid:{columns:5,rows:2,gutter:0},cell:{width:24,height:30}});
    const result=processedAnimationPack(source,pack);
    expect(validateAnimationPackV2(result).frameOrder).toHaveLength(9);
    expect(result.frames[5].sheetRect).toEqual({x:0,y:30,width:24,height:30});
    expect(result.frames[8].sheetRect).toEqual({x:72,y:30,width:24,height:30});
    expect(result.frameDurations).toEqual(Array(9).fill(143));
    expect(result.anchors[0]).toMatchObject({x:0.5,y:25/30});
    expect(result.qualitySummary.rank).toBe("unknown");
    expect(pack.frames[5].sheetRect.x).toBe(120);
  });
});
