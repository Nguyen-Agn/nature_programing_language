// Phần mở rộng chỉ làm một việc: khởi động `anature lsp` và nối nó với VS Code.
// Mọi hiểu biết về ngôn ngữ nằm trong chương trình anature và file language.toml.
const fs = require("fs");
const path = require("path");
const vscode = require("vscode");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

// Thứ tự tìm chương trình anature: thiết lập của người dùng, rồi bản vừa biên dịch
// trong cây mã nguồn (khi thư mục này được nối thẳng từ dự án), rồi các chỗ cài quen
// thuộc, cuối cùng là PATH.
function serverPath() {
  const configured = vscode.workspace.getConfiguration("anature").get("serverPath");
  if (configured && configured !== "anature") {
    return configured;
  }
  const project = path.dirname(fs.realpathSync(__dirname));
  const home = require("os").homedir();
  const candidates = [
    path.join(project, "target", "release", "anature"),
    path.join(project, "target", "debug", "anature"),
    // VS Code mở từ màn hình nền có thể không thấy PATH của terminal.
    path.join(home, ".local", "bin", "anature"),
    path.join(home, ".cargo", "bin", "anature"),
  ];
  return candidates.find((file) => fs.existsSync(file)) || "anature";
}

// Chạy `anature <lệnh> <tệp đang mở>` trong terminal của VS Code.
function runInTerminal(subcommand) {
  const editor = vscode.window.activeTextEditor;
  if (!editor || editor.document.languageId !== "anature") {
    return;
  }
  editor.document.save().then(() => {
    const terminal = vscode.window.terminals.find((t) => t.name === "Anature") || vscode.window.createTerminal("Anature");
    terminal.show(true);
    terminal.sendText(`"${serverPath()}" ${subcommand} "${editor.document.fileName}"`);
  });
}

exports.activate = function (context) {
  client = new LanguageClient(
    "anature",
    "Anature",
    { command: serverPath(), args: ["lsp"] },
    { documentSelector: [{ language: "anature" }] }
  );
  client.start().catch((error) => {
    vscode.window.showErrorMessage(
      `Anature: không khởi động được "${serverPath()} lsp" (${error.message}). ` +
        "Cài bằng `cargo install --path .` hoặc đặt anature.serverPath trong Settings."
    );
  });

  context.subscriptions.push(
    vscode.commands.registerCommand("anature.run", () => runInTerminal("run")),
    vscode.commands.registerCommand("anature.build", () => runInTerminal("build"))
  );
};

exports.deactivate = function () {
  return client ? client.stop() : undefined;
};
