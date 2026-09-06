import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WorkerRequests } from "./WorkerRequests";
import type { WorkflowAgent } from "@/core/workflow";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue(undefined) }));
it("routes a Manual approval to the exact worker and turn, without approving the parent", async () => {
  const user = userEvent.setup();
  const agent: WorkflowAgent = {id:"child",parentId:"main",role:"builder",title:"Criar arquivo",status:"waiting",createdAt:1,updatedAt:2,attempts:1,options:{account:"personal",model:"model",reasoning:null,mode:"build",approvalMode:"manual"},beadId:"task",handoff:null,error:null,activeTurnId:"child-turn",pendingQuestion:null,pendingApproval:{id:"write1",name:"write",args:{path:"src/test.ts",content:"example"},status:"pending",output:"",durationMs:0}};
  render(<WorkerRequests conversationId="root" projectPath="/project" agents={[agent]} drafts={new Map()} />);
  expect(screen.getByRole("region",{name:"Solicitação de Construtor"})).toBeInTheDocument();
  await user.click(screen.getByRole("button",{name:"Autorizar uma vez"}));
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("approve_workflow_tool",{conversationId:"root",agentId:"child",turnId:"child-turn",toolId:"write1",approved:true}));
});
