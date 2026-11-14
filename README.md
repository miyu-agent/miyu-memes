# miyu-memes

Miyu 的表情包软件包（编号 `memes`）：一个表情包库，每张图带描述和用法，AI 照意思从库里挑一张发，也能把聊天里看到的图存进来；也是接入通讯平台（im）的插件，能按群、私聊开关。

## 装

先装好 Miyu，再：

```sh
miyu pkg install memes
```

装上就跑，不用打开。能装在 `linux-x86_64`、`macos-aarch64`、`windows-x86_64` 上。装了多出这几样：

- AI 用的五件工具：`meme_search`、`meme_send`、`meme_save`、`meme_edit`、`meme_delete`。显示不了图的会话里，这个包当没开。
- 每一轮有一定概率提醒 AI 配一张表情包，设置键 `memes.reminder`、`memes.reminder_probability`、`memes.max_size_mb`。
- 后台页：`miyu pkg page memes`，在里面上传、搜、改、停用。
- im 的插件「表情包」，默认开着。
- 给别的软件包用的总线方法 `memes.add`、`memes.search`。

现成的图包：`miyu pkg install memes-shorin-collection`。怎么往库里加图、每项设置是什么、出了问题怎么办，见 [附加软件：表情包](https://github.com/miyu-agent/miyu/blob/main/docs/guide/addons.md#表情包-memes)。

## 开发

一个 Rust 程序 `miyu-memes`，只经协议和核心说话，不依赖核心的 crate。包文件夹是 `package/`：清单 `package.toml`、后台页 `page/`、给 im 的插件说明 `im/plugin.toml`。

要 Rust（版本照 `rust-toolchain.toml`，`rustup` 会自己装）。CI（`.github/workflows/ci.yml`）在 Linux、macOS、Windows 上跑：

```sh
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --locked
cargo test --locked --no-fail-fast
```

另查每个 `.rs` 不超过 500 行，测试也算。

在本机试装：编好程序放进包文件夹的 `bin/`，再把包文件夹拷成名叫 `memes` 的文件夹装上（包的编号照文件夹名定）：

```sh
cargo build --release --locked
mkdir -p package/bin
cp target/release/miyu-memes package/bin/
cp -R package /tmp/memes
miyu pkg install /tmp/memes
```

Windows 上程序带 `.exe`。发版推 `v<版本>` 的标签，`.github/workflows/release.yml` 一种系统打一个 `.miyu` 传进 Release，标签的版本照 `package/package.toml` 的 `version`（`Cargo.toml` 的版本跟着一起改）。

给模型看的字（工具说明、参数说明、工具结果、提醒）都记在 `docs/prompt-ledger.md`。

## 许可证

AGPL-3.0-or-later，见 `LICENSE`（2026-10-26 项目主人定许可证分层：核心和第一方程序用 AGPL-3.0-or-later）。

不是这个仓库自己写的东西照它们原来的许可证，AGPL 只管这里自己写的部分：依赖照各自的 `license`；收的第三方资源照各自的许可证，原文随附。表情包内容包（例如 miyu-memes-shorin-collection）是图和描述，照它们各自仓库的说明。
