<p align="center">
  <a href="README.md">English</a> | 简体中文
</p>

# Miyu 表情包（miyu-memes）

> <sub>本文档由 Anthropic 的 Claude Fable 5.2 生成。</sub>

这个仓库是 Miyu 的表情包软件包（编号 `memes`）。它提供一个表情包库，库里每张图片都带有描述和用法，AI 会根据对话的意思从库里挑一张图片发送，也能把聊天里看到的图片存进库里。它同时是通讯平台的插件，可以按群、按私聊分别打开或关闭。

## 安装

先安装 Miyu，然后运行：

```sh
miyu pkg install memes
```

安装以后软件包会自动运行，不需要手动打开。这个软件包可以安装在 `linux-x86_64`、`macos-aarch64`、`windows-x86_64` 上。安装以后会多出以下内容：

- AI 使用的五个工具：`meme_search`、`meme_send`、`meme_save`、`meme_edit`、`meme_delete`。在无法显示图片的会话里，这个软件包视为没有打开。
- 每一轮对话有一定概率提醒 AI 配一张表情包。相关的设置键有 `memes.reminder`、`memes.reminder_probability`、`memes.max_size_mb`。
- 后台页：运行 `miyu pkg page memes` 打开，你可以在里面上传、搜索、修改、停用表情包。
- 通讯平台的插件 `表情包`，默认打开。
- 供其他软件包调用的总线方法 `memes.add`、`memes.search`。

现成的图包可以用 `miyu pkg install memes-shorin-collection` 安装。怎么往库里添加图片、每项设置的含义、出了问题怎么处理，可以看 [附加软件：表情包](https://github.com/miyu-agent/miyu/blob/main/docs/zh/guide/addons.md#表情包-memes)。

## 开发

这个软件包是一个 Rust 程序 `miyu-memes`，只通过协议和核心通信，不依赖核心的任何 Rust 库（crate）。软件包文件夹是 `package/`，里面有清单 `package.toml`、后台页 `page/` 和给通讯平台的插件说明 `im/plugin.toml`。

开发需要 Rust，版本以 `rust-toolchain.toml` 为准，`rustup` 会自动安装这个版本。CI（`.github/workflows/ci.yml`）在 Linux、macOS、Windows 上运行以下检查：

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --locked
cargo test --locked --no-fail-fast
```

除此之外，还要检查每个 `.rs` 文件不超过 500 行，测试文件也算在内。

在本机试装的方法是：把编译好的程序放进软件包文件夹的 `bin/`，再把软件包文件夹拷贝成一个名叫 `memes` 的文件夹并安装（软件包的编号由文件夹名决定）：

```sh
cargo build --release --locked
mkdir -p package/bin
cp target/release/miyu-memes package/bin/
cp -R package /tmp/memes
miyu pkg install /tmp/memes
```

在 Windows 上，程序的文件名带 `.exe`。发布新版本时推送 `v<版本>` 的标签，`.github/workflows/release.yml` 会为每种系统打包一个 `.miyu` 文件，上传到发行版。标签的版本以 `package/package.toml` 里的 `version` 为准（`Cargo.toml` 里的版本要跟着一起修改）。

给模型看的文字（工具说明、参数说明、工具结果、提醒）都登记在 `docs/prompt-ledger.md` 里。

## 许可证

这个软件包的许可证是 AGPL-3.0-or-later，详细的见 `LICENSE`。

不是这个仓库自己编写的内容使用它们原来的许可证，AGPL 只适用于这个仓库自己编写的部分：依赖使用各自的 `license` 里写明的许可证；收录的第三方资源使用各自的许可证，原文随附。表情包的内容包（例如 `miyu-memes-shorin-collection`）是图片和描述，以它们各自仓库里的说明为准。
