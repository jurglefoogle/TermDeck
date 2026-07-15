export type TerminalSession = {
  id: string;
  name: string;
  cwd: string;
};

export type Workspace = {
  id: string;
  name: string;
  cwd: string;
  terminals: TerminalSession[];
  activeTerminalId: string | null;
};

export type LocatedTerminal = {
  terminal: TerminalSession;
  workspaceId: string;
  workspaceCwd: string;
};

export type EnvironmentInfo = {
  home: string;
  platform: string;
  shell: string;
  smokeTest: boolean;
};

export type TerminalInfo = {
  sessionId: string;
  generation: number;
  shell: string;
  cwd: string;
  pid?: number;
};

export type PtyEvent = {
  sessionId: string;
  generation: number;
  kind: 'output' | 'exit' | 'error';
  data?: string;
  exitCode?: number;
  signal?: string;
  message?: string;
};

export type DockPathInfo = {
  directory: string;
  suggestedName: string;
};

export type EditTarget =
  | { kind: 'workspace'; workspaceId: string; value: string }
  | { kind: 'terminal'; workspaceId: string; terminalId: string; value: string };
