import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { PixelProcessingPanel } from "./PixelProcessingPanel";

describe("native pixel processing suite controls", () => {
  const selected={id:"source",name:"hero.png",dataUrl:"data:image/png;base64,source"};
  it("submits grid recovery through the same command used by the agent", async () => {
    const execute=vi.fn().mockResolvedValue({});
    render(<PixelProcessingPanel selected={selected} grid={{columns:5,rows:2}} history={[selected]} frames={[]} execute={execute} activity={()=>undefined}/>);
    await userEvent.click(screen.getByRole("button",{name:"Preparar pixel art"}));
    await userEvent.click(screen.getByRole("button",{name:"Recuperar grade"}));
    expect(execute).toHaveBeenCalledWith("recover_grid",expect.objectContaining({sourceId:"source",options:expect.objectContaining({paletteColors:256,scale:1})}));
  });

  it("uses component extraction with preserved motion and a separate timeline action", async () => {
    const execute=vi.fn().mockResolvedValue({});
    render(<PixelProcessingPanel selected={selected} grid={{columns:5,rows:2}} history={[selected]} frames={[]} execute={execute} activity={()=>undefined}/>);
    await userEvent.click(screen.getByRole("button",{name:"Preparar pixel art"}));
    await userEvent.type(screen.getByLabelText("Células com poses"),"1, 2, 6");
    await userEvent.click(screen.getByRole("button",{name:"Extrair poses"}));
    expect(execute).toHaveBeenCalledWith("extract_poses",expect.objectContaining({sourceId:"source",addToTimeline:true,options:expect.objectContaining({columns:5,rows:2,alignment:"preserve",poseCells:[0,1,5]})}));
  });

  it("displays the real failure cause and does not submit work without an image", async () => {
    const execute=vi.fn().mockRejectedValue(new Error("Informe cellWidth e cellHeight."));
    const {rerender}=render(<PixelProcessingPanel selected={selected} grid={{columns:1,rows:1}} history={[selected]} frames={[]} execute={execute} activity={()=>undefined}/>);
    await userEvent.click(screen.getByRole("button",{name:"Preparar pixel art"}));
    await userEvent.click(screen.getByRole("button",{name:"Recuperar grade"}));
    await waitFor(()=>expect(screen.getByRole("alert")).toHaveTextContent("cellWidth"));
    rerender(<PixelProcessingPanel grid={{columns:1,rows:1}} history={[]} frames={[]} execute={execute} activity={()=>undefined}/>);
    expect(screen.getByRole("button",{name:"Recuperar grade"})).toBeDisabled();
  });

  it("shows actual processing state, preserves the original preview and surfaces geometry warnings",async()=>{
    const derived={id:"derived",name:"poses.png",dataUrl:"data:image/png;base64,derived"};
    const execute=vi.fn();
    const {rerender}=render(<PixelProcessingPanel selected={selected} grid={{columns:2,rows:1}} history={[selected,derived]} frames={[]} execute={execute} activity={()=>({status:"running",phase:"Extraindo poses completas"})}/>);
    await userEvent.click(screen.getByRole("button",{name:"Preparar pixel art"}));
    expect(screen.getByRole("status")).toHaveTextContent("Extraindo poses completas");
    expect(screen.getByRole("button",{name:"Extrair poses"})).toBeDisabled();
    rerender(<PixelProcessingPanel selected={derived} grid={{columns:2,rows:1}} history={[selected,derived]} frames={[]} execute={execute} activity={()=>({status:"completed",result:{sourceId:"source",historyIds:["derived"],report:{warnings:["Pose 1 ultrapassa a célula nominal; os pixels completos foram preservados."]}}})}/>);
    await waitFor(()=>expect(screen.getByAltText("Resultado do processamento")).toHaveAttribute("src",derived.dataUrl));
    expect(screen.getByAltText("Imagem original preservada")).toHaveAttribute("src",selected.dataUrl);
    expect(screen.getByText(/Pose 1 ultrapassa/)).toBeInTheDocument();
  });
});
