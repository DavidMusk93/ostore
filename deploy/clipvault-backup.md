# ClipVault → ostore 备份方案

> 依据 nmem 中的 ClipVault 决策整理：`per-host trx/`、**`backup/hosts/{hostId}/latest`**、
> **eventual consistency**、**never whole-db replace**；备份只做 hydrate/repair，**不当同步总线**。
> ostore 在本机（cc.guohuasun.com）作为 **第三地对象冷备**，不替代 iCloud/GDrive/夸克。

## 1. 角色划分

| 层级 | 位置 | 作用 |
|------|------|------|
| 主库 | 各 Mac `KEEPSAKE_HOME` | 日常读写 |
| 同步 | `sync/v1/trx/{hostId}/` + 云传输 | 多机 eventual consistency |
| 本地灾备 | iCloud / GDrive `cvbak` / 夸克 staging | 单机恢复、blob hydrate |
| **远程冷备（新增）** | **ostore** 键 `backup/hosts/{hostId}/latest/`（bucket 名 `clipvault`） | 跨机、跨云集中保留；**只镜像，不参与 sync** |

本机 `clipvault-session`（Postgres replica）**不是** Keepsake 文件库，**不能**直接 `pg_dump` 代替 Mac 上的 `backup/latest`。冷备数据源仍是 **Mac 上 ClipFlow 跑完 `POST /api/backup/run` 后的目录树**。

## 2. ostore 侧准备

在 `/etc/ostore.env` 启用 S3 面（与 JSON `/api` 的 `OSTORE_TOKEN` 分离）：

```bash
OSTORE_S3_ACCESS_KEY=clipvault-backup
OSTORE_S3_SECRET_KEY=<openssl rand -hex 32>
OSTORE_S3_REGION=us-east-1
OSTORE_S3_BUCKET=clipvault
```

建议 **单独根目录**，避免与 xstream 视频网盘混用：

```bash
# 可选：专用实例或专用 OSTORE_ROOT
OSTORE_ROOT=/srv/ostore-clipvault
```

`systemctl restart ostore` 后验证：

```bash
python3 scripts/s3_smoke.py http://127.0.0.1:8781   # 需 boto3；或通过 Caddy /ostore 前缀
```

对外暴露时：Caddy `handle /ostore/*` 反代 + TLS；**仅备份脚本持有 S3 密钥**，不下发到浏览器。

## 3. Mac 侧推送（推荐：S3 sync）

在 **每台** 已配置本地备份的 Mac 上，备份成功后再推送到 ostore：

```bash
export AWS_ACCESS_KEY_ID=clipvault-backup
export AWS_SECRET_ACCESS_KEY=<与 OSTORE_S3_SECRET_KEY 相同>
export AWS_DEFAULT_REGION=us-east-1
export AWS_ENDPOINT_URL=https://cc.guohuasun.com/ostore   # 按实际 Caddy 前缀调整

HOST_ID=$(defaults read com.davidmusk.clipflow hostId 2>/dev/null || hostname -s)
SRC="$HOME/Library/Application Support/Keepsake/backup/hosts/$HOST_ID/latest"

aws s3 sync "$SRC" "s3://clipvault/backup/hosts/$HOST_ID/latest/" \
  --only-show-errors \
  --no-progress
```

说明：

- S3 键即 ostore 相对路径：`backup/hosts/{hostId}/latest/...`（path-style，`clipvault` 为虚拟 bucket 名）。
- 使用 **`aws s3 sync`** 依赖 ostore 的 **ListObjectsV2 + PutObject + DeleteObject + HeadObject**；不要用整库覆盖式 rsync 到错误路径。
- **不要**在 sync 完成前把 ostore 当作「最新真相」；Mac 本地 + iCloud/GDrive 仍是第一恢复源。

### 挂到 ClipFlow 备份流水线

在 `POST /api/backup/run` 成功（`lastPhase=ok`）后触发上述 sync（LaunchAgent、`backup.json` 钩子或短 shell）。失败只告警，**不**回滚本地备份。

## 4. 备选：JSON API 镜像（无 AWS CLI）

与 README「mirror-up」相同语义，适合只有 `OSTORE_TOKEN`、不开 S3 的场景：

```bash
T=https://cc.guohuasun.com/ostore
AUTH="Authorization: Bearer <OSTORE_TOKEN>"
PREFIX="backup/hosts/$HOST_ID/latest"

curl -sf -H "$AUTH" "$T/api/list?path=$PREFIX&recursive=1&checksums=1" \
  | jq -r '.entries[] | select(.type=="file") | [.path,.sha256] | @tsv' | sort > /tmp/remote.tsv
# 本地对账后 PUT 变更、DELETE 多余（见 README Backup semantics）
```

S3 面更适合 **大 blob 目录** 与 **cron 增量**。

## 5. 恢复流程

1. **单机丢失**：先从 iCloud/GDrive `cvbak` 或夸克 staging 恢复 `latest/` + blobs。
2. **Mac 全灭、云盘不可用**：从 ostore 拉取对应 host 前缀：
   ```bash
   aws s3 sync "s3://clipvault/backup/hosts/$HOST_ID/latest/" "$RESTORE_DIR/"
   ```
3. 按 ClipVault 既有流程把 `latest` 挂回 `KEEPSAKE_HOME`，跑 verify / hydrate（`live/attach` 仍靠 trx 与 repair，**不要用 ostore 替代 sync**）。

## 6. 策略与禁忌（来自 nmem）

- **Write-once 键**：备份树内文件名以 CAS/hash 为主，避免同 key 覆盖导致前置 cache（若将来挂 cachey）Serving stale；ClipVault blobs 本身内容寻址，契合 ostore。
- **禁止**用 ostore 上的 `backup/latest` 做 **多机 sync 总线**（共享 `latest/` 已废弃，仅 per-host）。
- **禁止**「整库替换」式恢复；只恢复 **单个 `hostId` 的 `latest` 树**。
- **web_archive** 必须带 **文档闭包**（`blob_keys`）；冷备只是复制已闭包后的 `latest`，不在 ostore 上补归档逻辑。
- 密钥轮换：更新 `OSTORE_S3_*` 与 Mac 上 `AWS_*` 同步，JSON `OSTORE_TOKEN` 与 S3 密钥分开轮换。

## 7. 监控

```bash
curl -s -H "Authorization: Bearer $OSTORE_TOKEN" http://127.0.0.1:8781/api/metrics \
  | grep ostore_s3_
```

Mac 侧：备份 API 的 `lastPhase` + sync 退出码；可选 cron 每周 `aws s3 ls` 抽查 key 数量与本地 blobs 计数。

## 8. 与 xstream 网盘共存

当前生产 `OSTORE_ROOT=/root/xstream/data/videos` 面向 xstream `/files`。ClipVault 冷备建议：

- **方案 A（推荐）**：第二套 `ostore` 实例 + `/srv/ostore-clipvault` + 独立端口/env。
- **方案 B**：同一实例，键前缀 `clipvault/...` 与 `videos/...` 隔离；列表/权限仍共享一个 root，适合个人站、需自律前缀。
