// Đóng gói phần mở rộng thành một tệp .vsix tự chứa: chương trình anature được biên
// dịch và đặt vào server/, mã JavaScript được gộp vào dist/, rồi vsce tạo gói cho
// đúng nền tảng đang chạy lệnh này.
//
//   npm run package            gói cho máy hiện tại
//   npm run build              chỉ biên dịch và gộp, không tạo gói
const { execFileSync } = require("child_process");
const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..");
const project = path.join(root, "..");
const run = (cmd, args, cwd = root) => execFileSync(cmd, args, { cwd, stdio: "inherit" });
const bin = (name) => path.join(root, "node_modules", ".bin", process.platform === "win32" ? `${name}.cmd` : name);

// 1. Chương trình anature (máy chủ gợi ý và trình dịch).
run("cargo", ["build", "--release"], project);
const exe = process.platform === "win32" ? "anature.exe" : "anature";
fs.rmSync(path.join(root, "server"), { recursive: true, force: true });
fs.mkdirSync(path.join(root, "server"));
fs.copyFileSync(path.join(project, "target", "release", exe), path.join(root, "server", exe));
fs.chmodSync(path.join(root, "server", exe), 0o755);

// 2. Mã của phần mở rộng, gộp cùng thư viện thành một tệp.
run(bin("esbuild"), ["src/extension.js", "--bundle", "--platform=node", "--external:vscode", "--minify", "--outfile=dist/extension.js"]);

// 3. Gói .vsix cho nền tảng này. Chương trình anature là mã máy, nên mỗi nền tảng một gói.
if (process.argv.includes("--no-vsix")) {
  process.exit(0);
}
const target = `${process.platform}-${process.arch}`; // linux-x64, darwin-arm64, win32-x64, ...
// --skip-license: dự án chưa có file LICENSE. Khi đã chọn giấy phép, thêm file đó và
// trường "license" trong package.json rồi bỏ cờ này.
run(bin("vsce"), ["package", "--target", target, "--skip-license", "--out", `anature-${target}.vsix`]);
