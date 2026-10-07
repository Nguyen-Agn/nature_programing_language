// Phần mở rộng chỉ làm một việc: khởi động `anature lsp` và nối nó với VS Code.
// Mọi hiểu biết về ngôn ngữ nằm trong chương trình anature và file language.toml.
const fs = require("fs");
const os = require("os");
const path = require("path");
const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

// Thứ tự tìm chương trình anature:
//   1. thiết lập anature.serverPath của người dùng;
//   2. bản đóng gói sẵn trong phần mở rộng (thư mục server/);
//   3. bản vừa biên dịch trong cây mã nguồn, khi thư mục này được nối thẳng từ dự án;
//   4. các chỗ cài quen thuộc, rồi PATH.
function serverPath(context) {
  const configured = vscode.workspace.getConfiguration("anature").get("serverPath");
  if (configured) {
    return configured;
  }
  const exe = process.platform === "win32" ? "anature.exe" : "anature";
  const project = path.dirname(fs.realpathSync(context.extensionPath));
  const candidates = [
    path.join(context.extensionPath, "server", exe),
    path.join(project, "target", "release", exe),
    path.join(project, "target", "debug", exe),
    path.join(os.homedir(), ".local", "bin", exe),
    path.join(os.homedir(), ".cargo", "bin", exe),
  ];
  const found = candidates.find((file) => fs.existsSync(file));
  if (!found) {
    return "anature";
  }
  // Giải nén gói .vsix có thể làm mất quyền chạy của tệp.
  try {
    fs.chmodSync(found, 0o755);
  } catch {
    // Không đổi được quyền thì cứ thử chạy; lỗi sẽ được báo ở bước khởi động.
  }
  return found;
}

// Chạy `anature <lệnh> <tệp đang mở>` trong terminal của VS Code.
function runInTerminal(context, subcommand) {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== "anature") {
    return;
  }
  editor.document.save().then(() => {
    const terminal = vscode.window.terminals.find((t) => t.name === "Anature") || vscode.window.createTerminal("Anature");
    terminal.show(true);
    terminal.sendText(`"${serverPath(context)}" ${subcommand} "${editor.document.fileName}"`);
  });
}

exports.activate = function (context) {
  const command = serverPath(context);
  client = new LanguageClient("anature", "Anature", { command, args: ["lsp"] }, { documentSelector: [{ language: "anature" }] });
  client.start().catch((error) => {
    vscode.window.showErrorMessage(
      `Anature: không khởi động được "${command} lsp" (${error.message}). ` +
        "Hãy đặt anature.serverPath trong Settings thành đường dẫn tới chương trình anature."
    );
  });

  context.subscriptions.push(
    vscode.commands.registerCommand("anature.run", () => runInTerminal(context, "run")),
    vscode.commands.registerCommand("anature.build", () => runInTerminal(context, "build")),
    vscode.commands.registerCommand("anature.restart", () => client.restart())
  );
};

exports.deactivate = function () {
  return client ? client.stop() : undefined;
};
