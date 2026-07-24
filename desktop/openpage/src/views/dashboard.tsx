import { useAtomValue } from "jotai/react";
import { activePortAtom, sessionsAtom, activeExtensionsAtom, useSessionsSync } from "@/store/sessions";
import { useStreamSync, hasConsoleErrorsAtom } from "@/store/stream";
import { useActivitySync } from "@/store/activity";
import { useChatStatusSync } from "@/store/chat";
import { useMediaQuery } from "@/hooks/use-media-query";
import { Viewport } from "@/components/viewport";
import { ActivityFeed } from "@/components/activity-feed";
import { ChatPanel } from "@/components/chat-panel";
import { ConsolePanel } from "@/components/console-panel";
import { StoragePanel } from "@/components/storage-panel";
import { ExtensionsPanel } from "@/components/extensions-panel";
import { NetworkPanel } from "@/components/network-panel";
import { SessionTree } from "@/components/session-tree";
import { RecordingConsole } from "@/components/recording-console";
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from "@/components/ui/resizable";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";

export function DashboardPage() {
  const activePort = useAtomValue(activePortAtom);
  useStreamSync(activePort);
  useSessionsSync();
  useActivitySync();
  useChatStatusSync();

  const sessions = useAtomValue(sessionsAtom);
  const hasSessions = sessions.length > 0;
  const isDesktop = useMediaQuery("(min-width: 768px)");
  const hasConsoleErrors = useAtomValue(hasConsoleErrorsAtom);
  const activeExtensions = useAtomValue(activeExtensionsAtom);

  const sidePanel = (
    <Tabs defaultValue="chat" className="flex h-full flex-col">
      <div className="shrink-0 px-2 pt-1">
        <TabsList variant="line" className="h-7 w-full">
          <TabsTrigger value="recording" className="text-[11px]">录制</TabsTrigger>
          <TabsTrigger value="chat" className="text-[11px]">Chat</TabsTrigger>
          <TabsTrigger value="activity" className="text-[11px]">Activity</TabsTrigger>
          <TabsTrigger value="console" className="text-[11px]">
            Console
            {hasConsoleErrors && (
              <span className="ml-1 inline-flex size-1.5 rounded-full bg-destructive" />
            )}
          </TabsTrigger>
          <TabsTrigger value="network" className="text-[11px]">Network</TabsTrigger>
          <TabsTrigger value="storage" className="text-[11px]">Storage</TabsTrigger>
          <TabsTrigger value="extensions" className="text-[11px]">
            Extensions
            {activeExtensions.length > 0 && (
              <span className="ml-1 text-[9px] tabular-nums text-muted-foreground">{activeExtensions.length}</span>
            )}
          </TabsTrigger>
        </TabsList>
      </div>
      <TabsContent value="recording" className="min-h-0 flex-1 overflow-auto">
        <RecordingConsole />
      </TabsContent>
      <TabsContent value="activity" className="min-h-0 flex-1 overflow-hidden">
        <ActivityFeed />
      </TabsContent>
      <TabsContent value="console" className="min-h-0 flex-1 overflow-hidden">
        <ConsolePanel />
      </TabsContent>
      <TabsContent value="network" className="min-h-0 flex-1 overflow-hidden">
        <NetworkPanel />
      </TabsContent>
      <TabsContent value="storage" className="min-h-0 flex-1 overflow-hidden">
        <StoragePanel />
      </TabsContent>
      <TabsContent value="extensions" className="min-h-0 flex-1 overflow-hidden">
        <ExtensionsPanel />
      </TabsContent>
      <TabsContent value="chat" className="min-h-0 flex-1 overflow-hidden">
        <ChatPanel />
      </TabsContent>
    </Tabs>
  );

  if (isDesktop) {
    if (!hasSessions) {
      return (
        <div className="flex h-screen flex-col bg-background">
          <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
            <ResizablePanel id="sessions" defaultSize="15%" minSize="10%" maxSize="30%">
              <SessionTree />
            </ResizablePanel>
            <ResizableHandle />
            <ResizablePanel id="recording" defaultSize="85%">
              <div className="h-full overflow-auto">
                <RecordingConsole />
              </div>
            </ResizablePanel>
          </ResizablePanelGroup>
        </div>
      );
    }

    return (
      <div className="flex h-screen flex-col bg-background">
        <ResizablePanelGroup orientation="horizontal" className="min-h-0 flex-1">
          <ResizablePanel id="sessions" defaultSize="15%" minSize="10%" maxSize="30%">
            <SessionTree />
          </ResizablePanel>
          <ResizableHandle />
          <ResizablePanel id="viewport" defaultSize="55%" minSize="30%">
            <Viewport />
          </ResizablePanel>
          <ResizableHandle />
          <ResizablePanel id="activity" defaultSize="30%" minSize="15%" maxSize="50%">
            {sidePanel}
          </ResizablePanel>
        </ResizablePanelGroup>
      </div>
    );
  }

  return (
    <div className="flex h-screen flex-col bg-background">
      <Tabs defaultValue="recording" className="min-h-0 flex-1">
        <div className="shrink-0 px-2 pt-2">
          <TabsList className="w-full">
            <TabsTrigger value="sessions">Sessions</TabsTrigger>
            <TabsTrigger value="viewport">Viewport</TabsTrigger>
            <TabsTrigger value="recording">录制</TabsTrigger>
            <TabsTrigger value="activity">Activity</TabsTrigger>
          </TabsList>
        </div>
        <TabsContent value="sessions" className="min-h-0 overflow-hidden">
          <SessionTree />
        </TabsContent>
        <TabsContent value="viewport" className="min-h-0 overflow-hidden">
          <Viewport />
        </TabsContent>
        <TabsContent value="recording" className="min-h-0 overflow-auto">
          <RecordingConsole />
        </TabsContent>
        <TabsContent value="activity" className="min-h-0 overflow-hidden">
          {sidePanel}
        </TabsContent>
      </Tabs>
    </div>
  );
}
