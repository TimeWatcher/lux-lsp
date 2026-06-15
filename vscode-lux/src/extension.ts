import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  Middleware,
  ServerOptions,
  Trace
} from "vscode-languageclient/node";

type CommandResult = {
  kind: string;
  title: string;
  markdown: string;
  items: CommandItem[];
};

type CommandItem = {
  label: string;
  detail: string;
  description: string;
  markdown: string;
};

let client: LanguageClient | undefined;
let output: vscode.OutputChannel;

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  output = vscode.window.createOutputChannel("Lux");
  context.subscriptions.push(output);

  context.subscriptions.push(
    vscode.commands.registerCommand("lux.restartServer", async () => {
      await restartLanguageServer(context);
    }),
    vscode.commands.registerCommand("lux.openDocs", async () => {
      const docsUrl = config().get<string>("docs.url", "https://timewatcher.github.io/lux-docs-site/");
      await vscode.env.openExternal(vscode.Uri.parse(docsUrl));
    }),
    vscode.commands.registerCommand("lux.openGmodDocs", async (url?: string) => {
      const target = typeof url === "string" && url.length > 0
        ? url
        : config().get<string>("gmod.docsUrl", "https://wiki.facepunch.com/gmod/");
      await vscode.env.openExternal(vscode.Uri.parse(target));
    }),
    vscode.commands.registerCommand("lux.updateGmodApiDatabase", async () => {
      const args = config().get<string[]>("gmod.apiUpdateArgs", []);
      await runLuxcCommand("Update Garry's Mod API Database", ["gmod", "api", "update", ...args]);
    }),
    vscode.commands.registerCommand("lux.compileProject", async () => {
      await compileCurrentProject();
    }),
    vscode.commands.registerCommand("lux.formatDocument", async () => {
      await vscode.commands.executeCommand("editor.action.formatDocument");
    }),
    vscode.commands.registerCommand("lux.showModuleExports", async () => {
      await showServerResult("lux.showModuleExports", currentDocumentPosition());
    }),
    vscode.commands.registerCommand("lux.showActiveRealm", async () => {
      await showServerResult("lux.showActiveRealm", currentDocumentPosition());
    }),
    vscode.commands.registerCommand("lux.showGmodApiCoverage", async () => {
      await showServerResult("lux.gmodApiCoverage");
    })
  );

  await startLanguageServer(context);
}

export async function deactivate(): Promise<void> {
  if (client) {
    const current = client;
    client = undefined;
    await current.stop();
  }
}

async function restartLanguageServer(context: vscode.ExtensionContext): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
  await startLanguageServer(context);
  vscode.window.showInformationMessage("Lux language server restarted.");
}

async function startLanguageServer(context: vscode.ExtensionContext): Promise<void> {
  const serverOptions = resolveServerOptions(context);
  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "lux" },
      { scheme: "untitled", language: "lux" }
    ],
    synchronize: {
      fileEvents: [
        vscode.workspace.createFileSystemWatcher("**/*.lux"),
        vscode.workspace.createFileSystemWatcher("**/lux.toml")
      ]
    },
    middleware: markdownResourceMiddleware(context),
    outputChannel: output,
    revealOutputChannelOn: 4
  };

  client = new LanguageClient("lux", "Lux", serverOptions, clientOptions);
  client.setTrace(traceSetting());
  context.subscriptions.push(client);
  try {
    await client.start();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    output.appendLine(`failed to start Lux language server: ${message}`);
    vscode.window.showErrorMessage(`Failed to start Lux language server: ${message}`);
  }
}

function markdownResourceMiddleware(context: vscode.ExtensionContext): Middleware {
  return {
    async provideHover(document, position, token, next) {
      const hover = await next(document, position, token);
      rewriteHoverMarkdownResources(context, hover);
      return hover;
    },
    async provideCompletionItem(document, position, contextArg, token, next) {
      const result = await next(document, position, contextArg, token);
      rewriteCompletionMarkdownResources(context, result);
      return result;
    },
    async resolveCompletionItem(item, token, next) {
      const resolved = await next(item, token);
      rewriteCompletionItemMarkdownResources(context, resolved);
      return resolved;
    }
  };
}

function rewriteHoverMarkdownResources(context: vscode.ExtensionContext, hover: vscode.Hover | undefined | null): void {
  if (!hover) {
    return;
  }
  for (let index = 0; index < hover.contents.length; index += 1) {
    hover.contents[index] = rewriteMarkdownResourceValue(context, hover.contents[index]) as typeof hover.contents[number];
  }
}

function rewriteCompletionMarkdownResources(
  context: vscode.ExtensionContext,
  result: vscode.CompletionItem[] | vscode.CompletionList | undefined | null
): void {
  if (!result) {
    return;
  }
  const items = Array.isArray(result) ? result : result.items;
  for (const item of items) {
    rewriteCompletionItemMarkdownResources(context, item);
  }
}

function rewriteCompletionItemMarkdownResources(context: vscode.ExtensionContext, item: vscode.CompletionItem | undefined | null): void {
  if (!item) {
    return;
  }
  const documentation = rewriteMarkdownResourceValue(context, item.documentation);
  if (documentation !== undefined) {
    item.documentation = documentation as string | vscode.MarkdownString;
  }
}

function rewriteMarkdownResourceValue(context: vscode.ExtensionContext, value: unknown): unknown {
  let markdown: vscode.MarkdownString | undefined;
  if (value instanceof vscode.MarkdownString) {
    markdown = value;
  } else if (typeof value === "string") {
    markdown = new vscode.MarkdownString(value);
  } else if (isMarkupContentLike(value)) {
    markdown = new vscode.MarkdownString(value.value);
  }
  if (!markdown) {
    return value;
  }
  markdown.value = rewriteResourceUris(context, markdown.value);
  markdown.supportThemeIcons = true;
  markdown.supportHtml = true;
  markdown.isTrusted = true;
  return markdown;
}

function isMarkupContentLike(value: unknown): value is { value: string } {
  return typeof value === "object"
    && value !== null
    && "value" in value
    && typeof (value as { value?: unknown }).value === "string";
}

function rewriteResourceUris(context: vscode.ExtensionContext, markdown: string): string {
  return markdown.replace(/lux-resource:\/\/realm\/([a-z]+)\.svg/g, (_match, id: string) => {
    const uri = vscode.Uri.joinPath(context.extensionUri, "resources", "icons", `realm_${id}.svg`);
    return markdownUri(uri);
  });
}

function markdownUri(uri: vscode.Uri): string {
  return uri.toString().replace(/[\[\]]/g, "\\$&");
}

function resolveServerOptions(context: vscode.ExtensionContext): ServerOptions {
  const configured = config().get<string>("lsp.serverPath", "").trim();
  if (configured.length > 0) {
    return commandServerOptions(configured);
  }

  const bundled = findBundledBinary(context, "lux-lsp");
  if (bundled) {
    return commandServerOptions(bundled);
  }

  if (isCommandAvailable("lux-lsp")) {
    return commandServerOptions("lux-lsp");
  }

  if (config().get<boolean>("lsp.developmentCargoFallback", false)) {
    const cwd = config().get<string>("lsp.developmentWorkspace", "").trim() || workspaceRoot();
    return {
      command: "cargo",
      args: ["run", "-q", "-p", "lux-lsp"],
      options: { cwd }
    };
  }

  throw new Error("No lux-lsp binary found. Set `lux.lsp.serverPath`, install `lux-lsp` on PATH, or install a VSIX with bundled server binaries.");
}

function commandServerOptions(command: string): ServerOptions {
  return {
    run: { command, args: [] },
    debug: { command, args: [] }
  };
}

function traceSetting(): Trace {
  switch (config().get<string>("lsp.trace.server", "off")) {
    case "messages":
      return Trace.Messages;
    case "verbose":
      return Trace.Verbose;
    default:
      return Trace.Off;
  }
}

async function showServerResult(command: string, argument?: unknown): Promise<void> {
  const activeClient = client;
  if (!activeClient) {
    vscode.window.showWarningMessage("Lux language server is not running.");
    return;
  }
  const args = argument === undefined ? [] : [argument];
  const result = await activeClient.sendRequest<CommandResult>("workspace/executeCommand", {
    command,
    arguments: args
  });
  if (!result) {
    return;
  }
  if (result.items.length > 0) {
    const picked = await vscode.window.showQuickPick(
      result.items.map((item) => ({
        label: item.label,
        detail: item.detail,
        description: item.description,
        item
      })),
      {
        title: result.title,
        placeHolder: result.markdown.split("\n").find((line) => line.trim().length > 0)
      }
    );
    if (picked?.item.markdown) {
      showMarkdown(result.title, picked.item.markdown);
    } else if (picked) {
      showMarkdown(result.title, result.markdown);
    }
    return;
  }
  showMarkdown(result.title, result.markdown);
}

function showMarkdown(title: string, markdown: string): void {
  output.appendLine(`\n# ${title}\n${markdown}\n`);
  output.show(true);
}

function currentDocumentPosition(): { uri: string; line: number; character: number } | undefined {
  const editor = vscode.window.activeTextEditor;
  if (!editor) {
    return undefined;
  }
  return {
    uri: editor.document.uri.toString(),
    line: editor.selection.active.line,
    character: editor.selection.active.character
  };
}

async function compileCurrentProject(): Promise<void> {
  const manifest = await findWorkspaceManifest();
  if (manifest) {
    await runLuxcCommand("Compile Lux GMod Project", ["gmod", "build", "--manifest", manifest.fsPath]);
    return;
  }
  const editor = vscode.window.activeTextEditor;
  if (editor?.document.languageId === "lux" && editor.document.uri.scheme === "file") {
    await runLuxcCommand("Compile Lux File", ["compile", editor.document.uri.fsPath]);
    return;
  }
  vscode.window.showWarningMessage("Open a Lux file or workspace with lux.toml before compiling.");
}

async function runLuxcCommand(name: string, args: string[]): Promise<void> {
  const luxc = resolveLuxcPath();
  if (!luxc) {
    vscode.window.showErrorMessage("No luxc binary found. Set `lux.compiler.path` or install luxc on PATH.");
    return;
  }
  const terminal = vscode.window.createTerminal({
    name,
    cwd: workspaceRoot(),
    hideFromUser: false
  });
  terminal.show(true);
  terminal.sendText(shellQuote([luxc, ...args]));
}

function resolveLuxcPath(): string | undefined {
  const configured = config().get<string>("compiler.path", "").trim();
  if (configured.length > 0) {
    return configured;
  }
  const bundled = findBundledBinary(undefined, "luxc");
  if (bundled) {
    return bundled;
  }
  return isCommandAvailable("luxc") ? "luxc" : undefined;
}

function findBundledBinary(context: vscode.ExtensionContext | undefined, baseName: string): string | undefined {
  const platform = process.platform === "win32"
    ? "windows-x64"
    : process.platform === "darwin"
      ? process.arch === "arm64" ? "macos-arm64" : "macos-x64"
      : "linux-x64";
  const exe = process.platform === "win32" ? `${baseName}.exe` : baseName;
  const root = context?.extensionPath ?? path.resolve(__dirname, "..");
  const candidates = [
    path.join(root, "server", platform, exe),
    path.join(root, "bin", platform, exe),
    path.join(root, "bin", exe)
  ];
  return candidates.find((candidate) => fs.existsSync(candidate));
}

function isCommandAvailable(command: string): boolean {
  const paths = (process.env.PATH ?? "").split(path.delimiter);
  const extensions = process.platform === "win32"
    ? (process.env.PATHEXT ?? ".EXE;.CMD;.BAT").split(";")
    : [""];
  return paths.some((dir) =>
    extensions.some((extension) => fs.existsSync(path.join(dir, command + extension.toLowerCase())) || fs.existsSync(path.join(dir, command + extension.toUpperCase())))
  );
}

async function findWorkspaceManifest(): Promise<vscode.Uri | undefined> {
  const files = await vscode.workspace.findFiles("**/lux.toml", "**/{target,node_modules,.git}/**", 1);
  return files[0];
}

function workspaceRoot(): string {
  return vscode.workspace.workspaceFolders?.[0]?.uri.fsPath ?? process.cwd();
}

function shellQuote(parts: string[]): string {
  if (process.platform === "win32") {
    return parts.map((part) => `"${part.replace(/"/g, '\\"')}"`).join(" ");
  }
  return parts.map((part) => `'${part.replace(/'/g, "'\\''")}'`).join(" ");
}

function config(): vscode.WorkspaceConfiguration {
  return vscode.workspace.getConfiguration("lux");
}
