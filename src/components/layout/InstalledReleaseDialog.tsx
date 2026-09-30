import { useState } from "react";
import { JarvisLogo } from "@/components/JarvisLogo";
import { LazyChatMarkdown } from "@/components/chat/LazyChatMarkdown";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Dialog, DialogClose, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Separator } from "@/components/ui/separator";
import { APP_VERSION, displayVersion } from "@/core/app-update";
import { useDesktopLayout } from "@/hooks/use-desktop-layout";

export function InstalledReleaseDialog() {
  const { layout, updateLayout } = useDesktopLayout();
  const [dismissed, setDismissed] = useState(false);
  const [release] = useState(() => {
    const embedded = typeof __JARVIS_INSTALLED_RELEASE__ === "undefined" ? null : __JARVIS_INSTALLED_RELEASE__;
    return embedded?.version === APP_VERSION && embedded.notes.trim() ? embedded : null;
  });
  if (!release) return null;

  const open = !dismissed && layout.lastSeenReleaseVersion !== release.version;
  return <Dialog open={open} onOpenChange={next => {
    if (next || !open) return;
    setDismissed(true);
    updateLayout({ lastSeenReleaseVersion: release.version });
  }}>
    <DialogContent showCloseButton={false} className="dark flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden rounded-lg p-0 sm:max-w-2xl">
      <DialogHeader className="shrink-0 p-5 pr-10 sm:p-6 sm:pr-10">
        <div className="flex items-center gap-4">
          <JarvisLogo className="size-12 shrink-0" />
          <div className="flex min-w-0 flex-col gap-2">
            <DialogTitle>Novidades do Jarvis</DialogTitle>
            <DialogDescription>Veja o que mudou nesta versão.</DialogDescription>
            <div className="flex flex-wrap items-center gap-2">
              <span className="micro-label text-muted-foreground">Versão instalada</span>
              <Badge variant="outline" className="font-mono text-[10px]">{displayVersion(release.version)}</Badge>
            </div>
          </div>
        </div>
      </DialogHeader>
      <Separator />
      <section aria-label="Changelog da versão instalada" tabIndex={0} className="markdown-editor-prose min-h-0 overflow-y-auto overscroll-contain p-5 outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring sm:p-6 [&_a]:cursor-pointer!">
        <LazyChatMarkdown content={release.notes} />
      </section>
      <DialogFooter className="mx-0 mb-0 shrink-0 p-5 sm:p-6">
        <DialogClose render={<Button className="cursor-pointer" />}>Continuar</DialogClose>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}
