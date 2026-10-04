# KeepMyConfig

[![CI](https://github.com/OxygenAILab/KeepMyConfig/actions/workflows/ci.yml/badge.svg)](https://github.com/OxygenAILab/KeepMyConfig/actions/workflows/ci.yml)

> 在 CC Switch 切换第三方 API 供应商、以及重启 Codex/设备之后，仍然保住属于你的
> Codex 配置、MCP、Skills 与 Plugins/Addons。
>
> GitHub@OxygenAILab | OxygenAILab@StarsailsClover

KeepMyConfig 是一个 Rust 单文件工具，保护 Codex 配置中**真正属于用户**的部分：
手改的设置项、MCP 注册、Plugin/Marketplace 条目、受信任项目、Skills 与附件文件。
<!-- GitH   ub@OxygenA   ILab | Oxygen AILab@StarsailsClover   -->
它监视 `~/.codex/config.toml`，识别供应商切换工具造成的整文件覆写，并在不干扰刚切换到的
供应商的前提下，把你的配置合并回来。

英文文档：[README.md](./README.md)

---

## 为什么需要它

经 CC Switch v3.20.4 源码验证，它有两条互相独立的重写路径会吃掉用户配置：

1. **供应商切换**：把供应商保存的 `config.toml` 文本整份写回 live 文件。只有当该
   供应商 `meta.commonConfigEnabled = true` 时才会合并“公共配置”，而 UI 新建的
   供应商默认是 `false`。于是 `[plugins.*]`、`[marketplaces.*]`、`[desktop]`、
   `[windows]`、手工添加的 `[mcp_servers.*]`、额外的 `[projects.*]` 全部消失。
2. **MCP 同步**：`sync_enabled_to_codex` 用 CC Switch 自己的数据库重建
   `[mcp_servers]`，数据库里没有启用项时整表删除。直接写在 `config.toml` 里的
   MCP 会被删除。

KeepMyConfig 同时解决这两条路径：事后自动修复 live 配置，并可把受保护配置写回
CC Switch 自己的数据库，让它今后的每次切换都原生带上你的配置。

## 保护范围

| 受保护（用户所有） | 始终取 live（供应商标识） |
|---|---|
| `[mcp_servers.*]`（应用自管的 `node_repl` 除外） | `model`、`model_provider` |
| `[plugins.*]`、`[marketplaces.*]` | `[model_providers.*]`、`model_catalog_json` |
| `[desktop]`、`[windows]`、`[features]`、`[projects.*]` | `experimental_bearer_token`、`web_search` |
| 推理强度、上下文上限等设置项 | |
| 资产备份：`config.toml`、`AGENTS.md`、`skills/`、`plugins/`、`prompts/`、`rules/` | `auth.json` 只备份，绝不自动合并或恢复 |

既不保护也不恢复：`mcp_servers.node_repl` 与 `notify`。这两项由 Codex 桌面应用写入，
内含随版本变化的 runtime 路径（`runtimes\cua_node\<hash>\...`）；App 更新后 overlay 里的
副本即失效，而显式 `repair` 会无条件合并，等于把死路径写回去。若你手工维护并希望保护，
请把它从 `policy.toml` 的 `ignored` 中移除。

供应商标识被刻意排除，因此切换供应商仍然有效：模型跟随供应商，其余配置回归你自己。

## 安装

### 下载

从 [Releases](https://github.com/OxygenAILab/KeepMyConfig/releases) 下载
`keepmyconfig-x86_64-pc-windows-msvc.zip`，把 `keepmyconfig.exe` 放进 `PATH`。

### 从源码构建

```bash
git clone https://github.com/OxygenAILab/KeepMyConfig
cd KeepMyConfig
cargo build --release
# target/release/keepmyconfig.exe
```

需要 Rust 1.80 及以上。

## 快速开始

```powershell
# 1) 把当前满意的配置记为基线
keepmyconfig init

# 2) 后台守护（或配置开机自启，见文档）
keepmyconfig watch

# 3) 让保护在重启与 Codex 更新后继续生效（Windows）
keepmyconfig autostart install
# 也可显式选择机制：
# keepmyconfig autostart install --method task|startup

# 4) CC Switch 切换后检查；必要时手动修复
keepmyconfig status
keepmyconfig repair
```

配置已经丢了？从 CC Switch 或 Codex 的备份里找回：

```powershell
keepmyconfig init
keepmyconfig capture --from "$HOME\.codex\config.toml.bak-20261002-135358"
keepmyconfig repair
```

## 从根源上修复 CC Switch

`watch` 是兜底；`ccswitch adopt` 治本。它默认 **dry-run**，CC Switch 运行中拒绝写入
（除非 `--force`），并且一定先做数据库备份：

```powershell
keepmyconfig ccswitch inspect   # 只读概览
keepmyconfig ccswitch adopt     # 预览
keepmyconfig ccswitch adopt --apply
```

它在一个 SQLite 事务里完成三件事：

1. 把受保护覆盖层合并进 `settings.common_config_codex`；
2. 为每个 Codex 供应商设置 `providers.meta.commonConfigEnabled = true`；
3. 把受保护的 MCP 写入 `mcp_servers` 表并置 `enabled_codex = 1`。

`ccswitch backups` 列出快照；`ccswitch restore --backup <file>` 回滚数据库。

## 命令一览

| 命令 | 用途 |
|---|---|
| `init` | 创建存储并采集首个基线 |
| `capture [--from FILE]` | 有意修改后重建基线，或从备份文件恢复受保护键 |
| `status [--check] [--json]` | 健康度、漂移与 CC Switch 状态；`--check` 有漂移时退出码 2 |
| `diff` | 路径级对比 live 与基线 |
| `repair [--dry-run] [--check]` | 立即把覆盖层合并回 live |
| `watch [--once] [--dry-run]` | 持续检测并修复（或只跑一轮） |
| `autostart install\|uninstall\|status` | Windows 登录守护 + 周期性单次修复任务 |
| `backup --assets config,skills,plugins` | 版本化资产备份（`--link` 用硬链接） |
| `restore-assets [--from DIR]` | 恢复缺失文件；`--overwrite` 覆盖 |
| `ccswitch inspect\|adopt\|backups\|restore` | 可选的 CC Switch 数据库集成 |
| `doctor` | 环境与兼容性报告 |

完整参考：[docs/USAGE.zh-CN.md](./docs/USAGE.zh-CN.md)。

## 检测原理

只有同时满足“受保护路径消失”与“供应商托管键变化”，且几乎没有新增用户路径时，才会被
判定为供应商切换覆写。评分（balanced 模式阈值 4）还包括批量删除、文件缩小与新增用户
路径。另有两条针对应用更新的信号：`BROWSER_USE_CODEX_APP_VERSION` 变化且受保护路径
消失时必定视为整文件重写；一次丢掉 3 个及以上完整的 MCP/Plugin/Marketplace/Project
条目会显著加分。
路径。用户编辑与 Codex 自身更新会被采集为新基线，而不会被回滚。每次判定及证据都会写入
`~/.codex/.keepmyconfig/journal.jsonl`。

## 安全性

- 所有写入使用同目录临时文件 + rename。
- 每次修复前备份 live `config.toml`。
- 存储锁串行化并发 watch/单次运行。
<!-- Git  Hu b@  Ox   ygen AILab |   Ox   ygenAILab@StarsailsClo   ver   -->
- `auth.json` 绝不自动合并或恢复。
- 无网络访问、无遥测。

## 开发

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

发布构建：`scripts\build_release.ps1`。

## 许可

MIT

GitHub@OxygenAILab | OxygenAILab@StarsailsClover
