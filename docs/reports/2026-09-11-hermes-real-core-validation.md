# Hermes 真实核心迁移验证

范围：官方 v2026.9.7 的隔离验证与发布构建器修正；没有生产切换、数据库写入或外部消息发送。

## 已确认结果

- 官方 commit：`2237be355906fbe6065ce1815711eee52b2d646e`。
- Python 3.12 按 uv.lock 安装 messaging、wecom extras 后，真实 CLI `--help` 通过。
- `runtime/hermes/check_core_plugin_compatibility.py` 在临时 home、禁止 socket
  connect 的条件下验证 QiWe 使用真实 Hermes 基类，并完成 QiWe、WeCom、WeCom
  callback 平台注册。
- `uv run --with pyyaml --with pytest --with python-dotenv --python 3.12 python -m pytest runtime/hermes/tests -q`：21
  passed，6 subtests passed。
- `node tools/deploy/test-build-hermes-core-artifact.mjs`：CLI 成功/失败检查及非 Linux 构建拒绝通过；macOS 结果不代表 Linux 构建成功。
- `node tools/deploy/test-hermes-profile-registry.mjs` 通过。
- Linux amd64 artifact 构建成功，大小约 569
  MiB；在一个没有系统 Python 的独立容器中，artifact 自带 Python
  3.12.13 成功导入 aiohttp/defusedxml 并执行 Hermes CLI smoke。
- QiWe 303 项测试分别在旧的独立测试模式和真实 v2026.9.7 核心模式运行，均为 OK（1
  skipped）。真实核心首次运行有 3 个断言仍把 SessionSource 当字典读取；断言改为兼容对象属性后全部通过，运行代码没有同类读取。

## 定位与修正

1. 构建器固定写入 Linux 元数据，却没有验证构建主机和解释器。现在只允许 Linux
   x86_64、Python 3.11–3.13。
2. `hermes_cli_smoke=passed`
   原先没有对应的执行检查。现在在裁剪后的候选 venv 中运行隔离 CLI，失败即终止，不生成成功产物。
3. 原依赖导出遗漏 messaging、wecom extras；现在显式导出，并用
   `uv pip sync --require-hashes` 安装。
4. profile 数量原先为硬编码。现在加载并校验 registry；这仍只是静态契约，生产配置 parity 必须另做。
5. launcher 测试原先只复制解释器，uv 管理的 Python 无法定位标准库。夹具改用完整 venv 后，首次执行的 2 个失败消除，最终 21 项通过。
6. 进一步验证确认 `venv --copies` 仍通过 `pyvenv.cfg`
   依赖构建机标准库，不可作为可迁移 runtime。构建器现在只接受 uv 的完整可迁移 Python 分发包，并将解释器、标准库和依赖一起封装。

## 构建环境与剩余边界

Docker Hub 拉取 Python/Node 镜像曾出现 TLS record
version 错误。构建最终使用已有摘要固定的 Linux amd64 Node 镜像、只读系统公共 CA
bundle，以及单独下载并校验 SHA-256 的 uv 0.11.13。没有关闭 TLS 或哈希校验。

这些检查不证明现有本地 WeCom/Webhook/Kanban 行为已经迁移，也不证明七个 profile 已在新版本验收。迁移执行者须继续完成 Linux 产物、配置副本 parity、业务行为回放和首次切换回滚验证，然后才能切换生产。
