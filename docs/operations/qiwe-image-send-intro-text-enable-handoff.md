# 开启「日报/早报文字说明」生产操作说明（可直接转发）

> 给能登录生产服务器的同事。目标：让企微群里的**小满日报**和**二花早报**恢复「先发一条文字说明、再发海报图片」的形式。
>
> 改动已在仓库就绪（见文末「本次仓库改动」），本说明只做**生产开启**这一步。

## 一、背景（为什么会有这一步）

「发图片前先发一条文字说明」这个功能代码一直在，但被一个开关守着：

- 开关名：`QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED`
- 位置：sidecar 进程环境，写在 `/etc/qintopia/message-sidecar.env`
- 现状：**默认关闭**，而且从来没有任何脚本把它写进生产 env，所以最近只发图、不发文字。

把这一行写进 env 并重启 sidecar，功能就恢复。

## 二、要做的改动（一句话）

往 `/etc/qintopia/message-sidecar.env` 追加一行，然后重启 sidecar：

```bash
QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED=1
```

## 三、推荐做法：走评审过的 one-shot（不要手改）

仓库已新增一个专用 one-shot 目标 `qiwe-image-send-intro-text-enable`，它会：

- 只写上面这一行固定常量；
- 已存在且值正确就跳过（幂等）；值不对/重复就**失败关门**（不会硬改）；
- 不碰任何 chat id / 群 id / 数据库 hash / 其它配置。

**前提：这次仓库改动已合并并发布到生产 release**（脚本
`deploy/sidecar/scripts/enable-qiwe-image-send-intro-text-production.sh` 已上线）。

操作：在 GitHub 触发 workflow **`Run Production Runtime One-Shot`**，填：

```text
release_sha=<当前生产 release 的 40 位 SHA>
runtime_one_shot_target=qiwe-image-send-intro-text-enable
backfill_date=            （留空）
payload_sha256=           （留空）
approval=approved-production-qiwe-image-send-intro-text-v1
```

one-shot 只负责写 env，**不会自动重启 sidecar**（见第四步）。

## 四、重启 sidecar 让开关生效

sidecar 是启动时从 env 文件加载配置的，改了 env 必须重启进程才生效：

```bash
sudo systemctl restart qintopia-message-sidecar.service
sudo systemctl is-active --quiet qintopia-message-sidecar.service && echo "sidecar active"
```

确认 env 已写入：

```bash
grep '^QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED=1$' /etc/qintopia/message-sidecar.env
```

## 五、验证（下次发报时）

- 等到下一次**二花早报**（每天 08:10 Asia/Shanghai）或**小满日报**触发；
- 群里应当**先出现一条文字**（早报类似「二花早报 {date} 已生成，完整内容见卡片。」，日报是「小满日报来啦 …」开场白），**紧接着是海报图片**；
- 顺序是「文字在前、图片在后」。

## 六、风险与回滚

- **fail-closed 注意**：开关打开后，如果企微的 `sendHyperText`
  文字接口在生产不可用，sidecar 会**连图片也不发**（保护机制，宁缺毋滥）。所以开启后请盯一次实际发送。
- **回滚**：把那行从 env 删掉（或改成 `=0`），再
  `sudo systemctl restart qintopia-message-sidecar.service` 即可立即恢复「只发图」。

## 七、本次仓库改动（供 review / 追溯）

> **Erhua 路径已默认开启**：`二花早报` 卡片与 `小满日报` 海报实际由 Erhua 的 Hermes
> gateway 发出，它读取
> `/home/ubuntu/.hermes/profiles/erhua/.env`。该 profile 在每次激活时由
> `runtime/hermes/migrate_erhua_livecool_env.py` 生成 `.env`，现在会**默认追加**
> `QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED=1`（仅在缺失时补写；若已是 `=0`
> 等显式值则保留）。因此 Erhua 这一路无需再跑上面的 one-shot，重新部署/重新渲染也不会丢失该开关。回滚方式：把该行改为
> `=0` 后重新激活 profile 即可。
>
> 上面的 one-shot 只作用于主 sidecar 的 `/etc/qintopia/message-sidecar.env`，**不影响**
> Erhua 的 `.env`；两路相互独立。

新增并接入了这个 one-shot 目标，共 8 个文件：

| 文件                                                                     | 改动                                          |
| ------------------------------------------------------------------------ | --------------------------------------------- |
| `deploy/sidecar/scripts/enable-qiwe-image-send-intro-text-production.sh` | 新增：写 env 的 fail-closed 脚本              |
| `deploy/runner/qintopia-agent-os-deploy-runner`                          | 注册目标 + approval 映射 + 预演/执行两个 case |
| `deploy/runner/deploy-request.schema.json`                               | targets / approval 枚举加入新值               |
| `.github/workflows/run-production-runtime-one-shot.yml`                  | workflow 下拉选项 + 校验 case                 |
| `tools/deploy/test-production-runtime-one-shot-runner.mjs`               | 新增端到端用例（已通过）                      |
| `tools/deploy/check-deploy-runner.mjs`                                   | 同步各清单                                    |
| `AGENTS.md`、`docs/operations/production-runtime-one-shot-runbook.md`    | 边界与操作文档                                |

- approval 常量：`approved-production-qiwe-image-send-intro-text-v1`
- 本地 `node tools/deploy/test-production-runtime-one-shot-runner.mjs` → **passed**
- 注：`tools/deploy/check-deploy-runner.mjs` 在本机因内存不足被 OOM（exit
  137），与本次改动无关，CI 上会跑。
