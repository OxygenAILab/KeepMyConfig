# KeepMyConfig 使用参考

> GitHub@OxygenAILab | OxygenAILab@StarsailsClover

## 1. 目录结构

```
<CODEX_HOME>/                      # 默认 ~/.codex（或 $CODEX_HOME）
├── config.toml
└── .keepmyconfig/
    ├── policy.toml                # 托管/忽略规则、合并模式、资产策略
    ├── state.json                 # 基线哈希、计数、最近事件
    ├── baseline/config.toml       # 最近一次可信的完整配置
    ├── overlay/config.toml        # 基线中受保护的投影
    ├── journal.jsonl              # 追加式审计日志
    └── backups/
        ├── <ts>-pre-repair.toml
        ├── assets-<ts>/
        └── cc-switch-<ts>.db
```

全局参数：`--codex-home PATH`、`--store PATH`、`--json`。
环境变量：`CODEX_HOME`、`KMC_CODEX_HOME`、`KMC_STORE`、`KMC_CCSWITCH_DB`。

## 2. 策略文件

```toml
schema = 1
merge_mode = "overlay_wins"   # 或 "live_wins"
detection = "balanced"        # "strict" / "balanced" / "off"

managed = [                   # 供应商标识：不保护，永远取 live
  "model", "model_provider", "model_catalog_json",
  "base_url", "wire_api", "model_providers",
  "experimental_bearer_token", "web_search",
]

ignored = [                   # 应用自管高频变化：不保护也不恢复
  "mcp_servers.node_repl",
  "notify",
]

[assets]
dirs = ["skills", "plugins", "prompts", "rules"]
files = ["config.toml", "AGENTS.md", "requirements.toml"]
globs = ["config.toml.bak-*"]
backup_only = ["auth.json"]
exclude = ["**/node_modules/**", "**/.git/**", "**/target/**", "**/__pycache__/**", "**/*.zip", "plugins/cache/**"]
max_file_bytes = 52428800
```

- 规则是点分 TOML 路径；匹配路径**及其所有子路径**，如 `model_providers` 覆盖整表。
- `mcp_servers.node_repl` 与 `notify` 由 Codex 桌面应用写入，内含随版本变化的
  runtime 路径（`runtimes\cua_node\<hash>\...`）。App 更新后 overlay 里的副本即失效，
  而显式 `repair` 会无条件合并，因此保护它们等于把死路径写回去；两者默认忽略。
  若你手工维护并希望保护，请把它从 `ignored` 中移除。
- 含点或反斜杠的键在路径里带引号，例如
  `projects."c:\\work\\demo".trust_level`；可用 `projects.*` 匹配。
- `overlay_wins`（默认）在冲突时恢复你的值；`live_wins` 尊重新写入。
<!-- Gi tHub@OxygenAI   Lab |   Ox  ygenAILab@Starsail sClover -->
- `detection = "off"` 关闭 watch 自动修复；手工 `repair` 仍可用。

## 3. 日常流程

### 首次使用

```powershell
keepmyconfig init
keepmyconfig status
keepmyconfig watch
```

开机自启（无需管理员）：`Win+R` 输入 `shell:startup`，创建
`KeepMyConfig.cmd`：

```bat
@echo off
"C:\path\to\keepmyconfig.exe" watch --quiet
```

需要崩溃自动重启时，改用任务计划程序“登录时触发 + 失败重启”。

### 有意修改配置后

watch 运行时，用户编辑与 Codex 自身更新会被自动采集。若 watch 未运行，执行：

```powershell
keepmyconfig capture
```

### CC Switch 切换供应商后

watch 运行时会自动修复。手动检查：

```powershell
keepmyconfig status --check   # 有漂移时退出码 2
keepmyconfig repair --dry-run
keepmyconfig repair
```

### 已经丢失配置的抢救

```powershell
keepmyconfig init
keepmyconfig capture --from "$HOME\.codex\config.toml.bak-20261002-135358"
keepmyconfig repair
```

`--from` 会先经过同一套策略投影，只恢复用户键；供应商标识仍取 live。

## 4. 资产备份

```powershell
keepmyconfig backup                          # config.toml、config.toml.bak-*、auth.json
keepmyconfig backup --assets config,skills
keepmyconfig backup --assets all
keepmyconfig backup --assets all --link      # 硬链接，省空间
keepmyconfig backup --assets plugins --include_cache
```

恢复默认只补缺失文件：

```powershell
keepmyconfig restore-assets
keepmyconfig restore-assets --dry-run
keepmyconfig restore-assets --overwrite
keepmyconfig restore-assets --include-backup-only   # 连 auth.json 一起恢复
```

默认排除 `node_modules`、`.git`、`target`、`__pycache__`、zip 与
<!-- GitHub@Oxyge nAILab | Oxyg   enAILab@StarsailsClo  ver -->
`plugins/cache/**`；超过 50 MiB 的文件跳过并计入报告。

## 5. CC Switch 集成

```powershell
keepmyconfig ccswitch inspect
keepmyconfig ccswitch adopt            # 预览
keepmyconfig ccswitch adopt --apply    # 需先退出 CC Switch
keepmyconfig ccswitch adopt --apply --force   # 运行中也强制写入（不推荐）
keepmyconfig ccswitch adopt --no-mcp   # 不改 mcp_servers 表
keepmyconfig ccswitch backups
keepmyconfig ccswitch restore --backup "...\cc-switch-20261002-190000.db"
```

adopt 按表结构（`settings`、`providers`、`mcp_servers`）校验数据库而非死认版本号，
记录 `PRAGMA user_version`，并在一致备份之后用一个事务写完全部改动。

## 6. 检测评分

| 信号 | 分值 |
|---|---|
| 至少 1 个受保护路径消失 | +1 |
| 至少 3 个 | +1 |
| 至少 8 个 | +1 |
| 供应商托管键发生变化 | +2 |
| 无新增用户路径 | +1（1–5 个：0；更多：−1） |
| 文件缩小 ≥30% | +1 |

`balanced` 阈值 4，`strict` 阈值 3，`off` 从不自动修复。

## 7. 退出码与 JSON

| 退出码 | 含义 |
|---|---|
| 0 | 成功 / 健康 |
<!-- GitHub@Oxyg  enAILab | OxygenAILab@Sta rsailsC   lover -->
| 1 | 错误（含 doctor 结构性问题） |
| 2 | `--check` 发现漂移或覆写特征 |

所有报告类命令支持 `--json`；`journal.jsonl` 为 JSON Lines。

## 8. 常见问题

| 现象 | 处理 |
|---|---|
| `store is not initialized` | `keepmyconfig init` |
| `another process holds .../.lock` | 关闭另一个 watch/单次进程 |
| `cannot parse TOML` | 文件正在被写入；watch 会重试，或稍后 status |
| 修复把你不再想要的值带回来了 | 用 `capture` 重建基线，或改 `policy.toml` |
| CC Switch 仍然反复覆写 | `keepmyconfig ccswitch adopt --apply` |
| `auth.json` 不是最新 | 它被刻意排除在自动恢复之外；重新登录，或谨慎使用 `restore-assets --include-backup-only` |

## 9. 安全说明

- 无网络请求、无遥测。
- `auth.json` 默认不参与合并/恢复，因为 CC Switch 会按供应商合法轮换它。
- CC Switch 数据库写入必须显式 `--apply`，先备份，运行中除非 `--force` 否则拒绝。

GitHub@OxygenAILab | OxygenAILab@StarsailsClover
