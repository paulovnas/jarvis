import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from "@/components/ui/resizable";
import { ChatArea } from "@/components/chat/ChatArea";
import { Inspector } from "./Inspector";
import { AppSidebar } from "./Sidebar";

export function Home() {
  return (
    <div
      data-testid="home-shell"
      className="dark flex h-full min-h-0 w-full min-w-[1024px] overflow-hidden bg-[#282c34] font-sans text-[#abb2bf]"
    >
      <ResizablePanelGroup
        id="home-shell-panels"
        orientation="horizontal"
        className="h-full min-h-0 w-full flex-1"
      >
        <ResizablePanel
          id="home-sidebar-panel"
          defaultSize="22%"
          minSize="240px"
          maxSize="500px"
          className="h-full min-h-0 min-w-0"
        >
          <AppSidebar />
        </ResizablePanel>

        <ResizableHandle
          withHandle
          aria-label="Redimensionar barra lateral"
          className="cursor-pointer cursor-col-resize bg-[#3e4451] hover:bg-[#61afef]/50"
        />

        <ResizablePanel
          id="home-main-panel"
          defaultSize="50%"
          minSize="360px"
          className="h-full min-h-0 min-w-0"
        >
          <ChatArea />
        </ResizablePanel>

        <ResizableHandle
          withHandle
          aria-label="Redimensionar inspector"
          className="cursor-pointer cursor-col-resize bg-[#3e4451] hover:bg-[#61afef]/50"
        />

        <ResizablePanel
          id="home-inspector-panel"
          defaultSize="28%"
          minSize="360px"
          maxSize="550px"
          className="h-full min-h-0 min-w-0"
        >
          <Inspector />
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  );
}

export default Home;
