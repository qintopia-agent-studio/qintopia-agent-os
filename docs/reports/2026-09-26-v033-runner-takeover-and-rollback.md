# v0.3.3 Runner 首次接管与回退

日期：2026-09-26。状态：技术方案待负责人评审；本报告不批准生产请求。

2026-09-27 独立评审后，文末「最终推荐与实施边界」为本报告当前结论。前面的首次接管证据仍有效；已替代的 13 文件方案不再作为实施清单。文中的生产只读核对均为 2026-09-26 历史证据；本轮报告修订没有访问生产。

## 依据与现状

已读 `README.md`、`AGENTS.md`、`docs/README.md`、当前 roadmap、change
routing、programming guardrails、server change policy、`deploy/runner/README.md`、
`docs/operations/production-deploy-runner.md`、相关 workflow、runner、schema、restart
rules、检查器与历史 bootstrap 报告。只读参考并行工作树的岸岸生产核查报告及
`anan-production-rollout.md`，不在本分支修改它们。2026-09-26 的生产取证仅用管理员给定 SSH 做脱敏只读核对；本轮未连接生产。

`v0.3.3` 已发布，精确 SHA 为 `62f403e3c5e615c10b6a0577954626664fcb22db`。GitHub 部署 run
`36206145717` 的构建与产物上传/核验成功，签名请求 `deploy-20260926T005852Z-62f403e3c5e6`
已发送，等待回执步骤失败。旧 runner 在提升前拒绝六目标请求，固定错误为
`invalid deploy request: restart_targets contains unsupported or duplicate entries`。六项目标没有重复；旧 runner 只缺
`hermes-anan`。该请求不能重试，也不能删掉岸岸目标当作 v0.3.3 已部署。

2026-09-26 只读服务器复核：`current=16e8d56b98001579c6288ba13199b80d6d3dfc74`，
`previous=83d694f2c3bc21fd78a73d25da3197379e2a14d5`，目标 `62f403e`
目录不存在。当前 manifest 为 `release_sha=commit_sha=deploy_bundle_sha=16e8d56b`、
`runtime_sha=83d694f2`、`previous_sha=83d694f2`；实际主 sidecar 制品为当前获准的三项 Huabaosi 特征，QiWe
companion 存在。这是既有混合版本契约，不是漂移。当前 runner 与 smoke 均不含
`hermes-anan`。原回执记载未提升且无需回退。

## 首次接管路径

候选为两次**不同身份**的签名请求，不能重放失败请求。第一步只安装能认识新目标且具备精确前次树回退事务的受审修复 bundle；第二步才以完整六目标部署该修复 Release。由于 v0.3.3 不含此回退事务，不能让它作为第二步的最终
`current`，否则会再次失去可执行的手动回退入口。它的失败需由后续修复 Release 接替，不能声称 v0.3.3 原请求成功。下文以
`R` 表示批准后生成并审阅的修复提交/制品 SHA；在 `R`
固定前不得发生产请求。前提是负责人分别批准精确生产请求，并在第一步前核对最新可信成功回执、当前/前次指针、旧 runner、COS 中
`83d694f2` 主/伴随运行制品与 `R`
bundle 的签名和摘要。P 对应回执为失败，不能把对象存在或旧树指针当作成功部署证明；R 产物尚不存在。

2026-09-26 再次只读核对：生产 O 的 runner、poller、promoter、rollback 脚本与 Git 提交
`16e8d56b` 的对应文件 SHA256 完全相同。使用服务器既有 COS
SDK 在进程内流式读取对象、计算 SHA256，未把包落地或输出配置：

| 已有制品                 | COS 内容 SHA256                                                    | 校验表预期 SHA256                                                  | 结果 |
| ------------------------ | ------------------------------------------------------------------ | ------------------------------------------------------------------ | ---- |
| P 主 sidecar tar         | `dbd8d029ba9a2754e697bb35553cebf79fecec4032334a602458e3dbe8f4c03e` | `dbd8d029ba9a2754e697bb35553cebf79fecec4032334a602458e3dbe8f4c03e` | 相符 |
| P QiWe companion tar     | `5dafbf5962aef55b07a43bde9af69cb1dfe5f78b210b5b434099ede6a5c6f851` | `5dafbf5962aef55b07a43bde9af69cb1dfe5f78b210b5b434099ede6a5c6f851` | 相符 |
| O deploy bundle tar      | `66fd3807c6aa1f3a0943c13779d7a4035abb81b4fa3580f5acbd072bd53aca7d` | `66fd3807c6aa1f3a0943c13779d7a4035abb81b4fa3580f5acbd072bd53aca7d` | 相符 |
| v0.3.3 deploy bundle tar | `dbf6bf9b82e295c51fa07ae433f277c2f7d3f4400a04c1bede5c6a5065e05177` | `dbf6bf9b82e295c51fa07ae433f277c2f7d3f4400a04c1bede5c6a5065e05177` | 相符 |

P 两份 `artifact-manifest.json` 的 COS 摘要分别为
`e5a7aa0cd1ee7689117657d363f04846b6bf0b3e072ad652a77acee0bad24a61` 和
`a803ebd92ffd401870d5efc7f03eec8db58ca1689a4ebdbe35bcd5746ee4725e`，也与各自 COS
`SHA256SUMS` 相符。三份旧校验表均与已装树字节一致；O bundle manifest不列入其
`SHA256SUMS`，但 COS 与已装树摘要同为
`c19558ae9ade6fe4f711adcec36eb186fc2ee38fa830b39f3580d8bede15fa8a`。v0.3.3
bundle 内的 installer、smoke、poller、runner、runner
unit 与 systemd 渲染脚本均存在且模式符合打包预期。这只能证明现成 v0.3.3 包的文件事实，不能替代未来 R 包的核验。

1. 固定过渡候选：`commit_sha=16e8d56b98001579c6288ba13199b80d6d3dfc74`，
   `runtime_sha=83d694f2c3bc21fd78a73d25da3197379e2a14d5`， `deploy_bundle_sha=R`，
   `release_sha=70e7984fab92ddab956009585212d0e9729767b5`（独立祖先提交，只作不可变过渡目录身份，服务器上目前不存在），
   `release_scope=[deploy-bundle]`，`restart_targets=[qintopia-system-services]`，
   `rollback_on_smoke_failure=true`。这是单独的 runner 接管事务，不是缩减 v0.3.3 的目标。不能标记为
   `legacy_runner_bootstrap`：该 Huabaosi 特征兼容模式要求最近可信成功回执的四个 SHA 相同，且已有批准不涵盖岸岸首次接管。
2. 用现有 `workflow_dispatch` 从 `master` 先发 `dry_run=true`，比对签名请求与
   `dry_run_succeeded` 回执的全部身份字段、目标、范围和
   `promoted_current=false`；任何不符即停。独立批准后用相同固定元组发新的
   `dry_run=false` 请求，要求
   `succeeded`、`current=70e7984`、`previous=16e8d56b`、新 runner 白名单含岸岸。同步失败时核验自动回退回执与原指针；进程中断或结果未知时停机核对 O/P、O/O 或 T/O，不重放原请求。
3. 对修复 Release 的完整六目标元组另发**新的**正常请求，先 dry run 后 live，
   `commit_sha=runtime_sha=deploy_bundle_sha=release_sha=R`，scope 依发布契约，六目标仍为
   `qintopia-system-services,hermes-erhua,hermes-xiaoman,hermes-silaoshi,hermes-huabaosi,hermes-anan`。live 后应为
   `current=R`、`previous=70e7984`。岸岸排空须取得新鲜零工作快照，最多等待 600 秒；超时暂缓并取消本次排空，不强杀、不补跑。

过渡 dry run 尚未实际执行；第一步旧 runner 对未来 R
bundle 的完整离线模拟、最新可信成功回执绑定及 R 产物可用性仍须验证。因此以上是待审执行路径，不是就绪结论。
`legacy_runner_bootstrap` 既有约束不得放宽或改名借用。

### 旧 O 隔离模拟

用精确 O 提交归档到 `/tmp`，运行旧版
`test-deploy-runner-promotion.mjs`、`test-promote-release-tree.mjs`、
`test-release-systemd-rollback.mjs`，均通过。额外一次性 fixture 使用模拟 O/P 指针和测试密钥签名请求：六目标在调用 promoter 前报
`restart_targets contains unsupported or duplicate entries`；
`commit=O,runtime=P,bundle=模拟 R,release=T`
且只重启 system 的请求通过旧 runner 验证并进入模拟 promoter。此结果证明请求合同和旧 runner 白名单的边界，未验证未来 R 制品的安装、实际 systemd 单元或生产 smoke。

旧 O
promoter 的真实顺序是：在 staging 下载主/伴随运行制品和 bundle、组装 manifest、校验树；把完整 staging 目录移到 T；先写
`previous=O`，再写
`current=T`；之后旧 runner 才调用 T 的 installer、smoke 并写回执。对旧 promoter 在这三个边界注入
`SIGKILL`：

| 中断点         | T 目录 | current / previous | 实际含义                                                 |
| -------------- | ------ | ------------------ | -------------------------------------------------------- |
| T 目录移动前   | 不存在 | O / P              | 指针未变；核对状态后停止，不需要新指针恢复机制           |
| 写 previous 后 | 完整   | O / O              | 原脚本未写 current；旧 rollback primitive 拒绝相同两指针 |
| 写 current 后  | 完整   | T / O              | 指针完成，installer/smoke 及回执尚未确认                 |

未中断的同一 fixture 得到 T/O。旧 runner 现有 fixture 的 installer 失败及 smoke 失败会进入同步自动回退；`SIGKILL`
不运行错误处理、installer 补偿或结果写入。因此 O/O、T/O 是首次接管必须覆盖的中断状态；T 未落盘时不存在本次指针改变。固定 T 入口可在 O/O 与 T/O 状态从完整 T 树运行，但必须先核验其 bundle 和原签名过渡请求。

另在独立 Ubuntu 24.04 Linux 容器对 GNU `ln -sfn`
的命令内部窗口做系统调用核查。容器 coreutils 为 `9.4-3ubuntu6.3`；生产为
`9.4-3ubuntu6.1`，生产 release 目录在 ext4 上。容器成功轨迹先以 `symlinkat`
创建同目录随机名临时链接，再用单次 `renameat` 覆盖已有 `current`，没有先删除目标。将
`renameat` 入口定点延迟并 `SIGKILL` 后，旧 `current`
仍指向 old，随机名临时链接指向 new。因此这一命令内部中断不产生缺失的 current/previous 指针，无需扩大上表的允许恢复状态；残留临时链接须作为证据核对，不自动清理。

两包修订之间的 Ubuntu changelog 只列出 `du` 和 `sort` 的代码变更，没有 `ln`
变更；但实验使用 arm64 容器二进制，不是生产 x86_64 `9.4-3ubuntu6.1`
二进制的逐字节执行。结论基于相同 coreutils
9.4 逻辑与同目录原子 rename 语义，不声称覆盖 Linux 内核或存储故障。最新只读服务器复核仍为 O/P，T 目录不存在。

脱敏复核命令如下；均只作用于 `docker run --rm` 的独立容器临时目录，`old/new/current`
是模拟路径，不是生产 Release 指针：

```bash
docker run --rm -i --cap-add SYS_PTRACE ubuntu:24.04 sh -s <<'SH'
set -eu
apt-get update -qq
apt-get install -y -qq strace >/dev/null
dir=$(mktemp -d)
mkdir "$dir/old" "$dir/new"
old="$dir/old"
new="$dir/new"
current="$dir/current"
trace="$dir/trace"
ln -s "$old" "$current"
strace -o "$trace" -e trace=symlinkat,renameat ln -sfn "$new" "$current"
cat "$trace"
ln -sfn "$old" "$current"
strace -o "$trace" -e trace=symlinkat,renameat \
  -e inject=renameat:delay_enter=10s \
  sh -c 'echo $$ > "$1/pid"; exec ln -sfn "$1/new" "$1/current"' sh "$dir" &
tracer=$!
while [ ! -s "$dir/pid" ]; do sleep 0.05; done
sleep 1
kill -9 "$(cat "$dir/pid")"
wait "$tracer" || true
cat "$trace"
readlink "$current"
find "$dir" -maxdepth 1 -type l -printf '%f -> %l\n'
SH
```

成功轨迹的关键 syscall 为 `symlinkat(new, temporary)=0`、
`renameat(temporary, current)=0`；定点中断轨迹为
`symlinkat(new, temporary)=0`、`renameat(temporary, current)=?`、
`killed by SIGKILL`，随后 `readlink current=old`。脱敏证据记录在本报告及
`docs/reports/README.md` 索引；容器内 `/tmp` trace 随 `--rm`
销毁，未保存服务器配置、凭据或原始日志。生产只读核对命令为
`ln --version`、`dpkg-query -W coreutils`、
`findmnt -no FSTYPE --target /home/ubuntu/qintopia-agent-os-releases`。

## 最终推荐与实施边界

### 决定与证据界限

采用**普通混合 exact-previous 签名回退 + 有限维护恢复**。正常回退仍由受审 `master` 上的
`rollback-production.yml`、`production`
环境、HMAC 签名新请求、COS 队列、runner 锁和签名结果完成。原双 tag 回退路径的已发布状态、可达性、制品验证均保留。

硬中断由受审的固定不可变入口处理有限状态，不日常编排 R→T→O/P 两跳。永久通用 recovery 请求协议、workflow 自动发现历史成功回执、选择性 bundle 格式不是默认必做项。此为技术建议，不等于具体 CI 范围已批准。

`O=16e8d56b98001579c6288ba13199b80d6d3dfc74`，
`P=83d694f2c3bc21fd78a73d25da3197379e2a14d5`，
`T=70e7984fab92ddab956009585212d0e9729767b5`，`R`
是未来受审修复提交。最近只读证据为 O/P，T 不存在、R 尚未生成。O 的原请求和回执签名有效且状态
`succeeded`；P 的请求和回执签名有效但状态是
`failed`。P 的旧树和 COS 摘要可作为精确恢复材料，不可写成成功部署证明。旧 O 中断模拟与“请求可表达、runner 拒绝”的本地模拟，只定位边界；未模拟未来 R 制品、混合回退、真实 systemd 沙箱或生产 dry
run。

### 普通混合回退的执行合同

过渡 T 的原 manifest 固定
`commit_sha=O,runtime_sha=P,deploy_bundle_sha=R,release_sha=T`、
`release_scope=[deploy-bundle]`、`restart_targets=[qintopia-system-services]`、
`previous_sha=O`。修复 R 成功后是 R/T。新的 R→T 回退请求由生产 workflow 签名，`release_rollback.expected_current_sha=R`、
`expected_previous_sha=T`；四 SHA 和 artifact
profile 必须与 T 原 manifest 完全一致。**请求的 `restart_targets`
是此次经审批的动作范围**，可包含岸岸；T 原 manifest 的单 system 目标是建树历史，不得重写来匹配此次请求。

workflow 应提供互斥的双 tag 与混合精确前次输入模式。混合模式只接受受审的 R current
Release 身份和完整 T 四元组，核对提交在
`origin/master`、固定 profile、固定 scope、目标 allowlist、COS 主/伴随 runtime 与 bundle 的 manifest 和
`SHA256SUMS`，然后沿现有 production 保护及签名创建器发新请求；不接受任意历史目录或单独一个 target
SHA。现有 schema 已有四 SHA 与
`release_rollback`，创建器已接受不同四 SHA；`wait-deploy-result.sh`
仍验证这次新回执，不需为了普通混合回退增加历史 ID 自动发现。旧双 tag 路径的全部验证不削弱。

runner 在现有 `validate_request()` 验签和字段校验后，仅在 `release_rollback`
的混合 exact-previous 分支放开“四 SHA 均等于 T”的限制。它须在 `deploy.lock` 下用
`capture_original_release_lineage()`、`validate_requested_release_rollback_lineage()`
核对 R/T 与 R manifest 的 `previous_sha=T`，再将 T
manifest 四元组、scope、profile、`previous_sha=O`
与签名请求和已验证 COS 制品逐项匹配，并复验 T 不可变树的路径、模式、内容，以及两个运行包与 bundle 的校验表。O 树及 installer 同样须先验证。

只有**原制品身份**与**此次动作授权**各自成立，才允许进入专属回退分支；普通同 SHA 发布仍走
`promote-release.sh` 的原检查。

专属分支不能调用现有普通 `promote-release.sh` 同 SHA 复用：它在既有目录比较
`restart_targets`，会因 T 原单 system 与此次动作目标不同而拒绝。在锁内、写持久意图后，专属分支调用已验证 release 内的
`rollback-release.sh --expected-current-sha R --expected-previous-sha T --restore-previous-sha O`。该原语依次写
`rollback-from=R`、`current=T`、`previous=O` 并安装 T 单元；省略
`--restore-previous-sha O` 会得到 T/R，与 T
manifest 谱系冲突。随后用**此次签名请求**的目标集运行 T 的 smoke，岸岸须先完成排空才重启。仅在指针、T
manifest、installer、目标 smoke 和签名回执一致时记成功。同步失败由专属补偿按持久意图恢复 R/T、运行 R
installer 和获批目标 smoke；不能套用 `run_promotion()` 后现有 `promoted_current`
的正向失败分支。进程死亡或结果不明转入下文有限维护路径，不重放原回退请求。

### 岸岸排空与最小写权限

当前 `smoke-release.sh` 的 `hermes-anan` 分支以 `runuser -u ubuntu` 在**同一 runner
service 沙箱**内调用 `runtime/hermes/restart_anan.py`。该脚本通过官方
`gateway.drain_control.write_drain_request(home=PROFILE)` 写入
`/home/ubuntu/.hermes/profiles/anan/.drain_request.json`。官方 `v2026.9.21` 的
[`gateway/drain_control.py`](https://github.com/NousResearch/hermes-agent/blob/v2026.9.21/gateway/drain_control.py)
调用
[`utils.atomic_json_write`](https://github.com/NousResearch/hermes-agent/blob/v2026.9.21/utils.py)：它在**同一 Profile 目录**以
`mkstemp` 建临时文件、fsync、rename 替换 marker； `clear_drain_request()`
从同目录 unlink。只给 marker 文件 `ReadWritePaths`
不足以支持创建、rename 和清除；直接给整个 runner `/home/ubuntu/.hermes/profiles/anan`
写权限，则把凭据、会话等目录也暴露给通用部署进程。现 runner unit 的
`ProtectHome=read-only` 且写路径不含岸岸， `runuser` 与 `daemon-reload`
均不会解除**当前进程**的 mount namespace。

推荐保持 runner unit 的 `ProtectHome=read-only`，只新增一个受审固定
`qintopia-agent-os-anan-drain-restart.service`：installer 将其渲染、安装为已验 release 的绝对路径，`User=ubuntu`，固定
`ExecStart` 为该 release 的 `restart_anan.py` 和官方 `v2026.9.21` Python，不从 `current`
解析脚本；保留 `NoNewPrivileges`、`PrivateTmp`、`ProtectSystem=strict`、
`ProtectHome=read-only`，仅对此**一次性服务**增加
`ReadWritePaths=/home/ubuntu/.hermes/profiles/anan`，并固定 user
systemd 运行环境。安装后核对有效 `ExecStart`
与已验 release 身份，且有效启动超时须晚于 helper 完整 600 秒排空及清理时间；超时不得强杀 gateway。
`smoke-release.sh`
只启动并等待该固定 unit 的退出结果，不接受任意命令、Profile 路径或服务名。

这个父目录写权限是现有官方同目录原子协议所需的最小挂载范围，仍能触达同目录其他文件。可信脚本和短生命周期是必要约束，不能宣称其余文件不可写。若该权限边界不获技术审查或真实 Linux 验证，岸岸 live 重启即被阻断；不能改官方核心来绕过。

保持现有 `restart_anan.py` 合同：已有 drain 则暂缓；新鲜 gateway
PID、两次不同更新时间的零工作快照且间隔至少两秒才重启；最长 600 秒，超时取消**本次**标记并暂缓，不强杀、不补跑、不删除他人的标记。服务失败、标记清理失败和 user
systemd 失联均不得签发成功回执。后续实现须在真实 Linux
systemd 下以有效属性和实际 marker 创建/rename/清除、沙箱外拒写验证，不能以 macOS、容器内 root 或测试替身代替。

### 首次 live 前隔离与一次性消费

旧 O poller 不持
`deploy.lock`，且在 runner 执行后才上传结果、归档请求。若仍由普通 timer 自由轮询，O→T 首次 live 的
`SIGKILL` 可能让旧 O poller 在回执缺失时再次读同一个 COS
pointer。**有限恢复必须在首次 live 之前预置**，不能中断后才尝试安装 hold。

生产预检阶段先从 COS 取得已审 R
bundle，核对其 manifest、`SHA256SUMS`、来源 SHA 和内容清单，固定在 root-owned 不可变暂存目录；其中包含受审的
`run-fixed-takeover-request.sh`、新版 poller、hold
drop-in 和维护 helper。固定 launcher 的 `prepare` 阶段先停用普通 deploy timer，等待旧 O
service/在途 poller 自然退出并确认没有锁持有者，再持久写 hold、安装固定 drop-in、`daemon-reload`
并验证普通 service 被 condition 阻断。此时才允许从现有
`deploy-production.yml workflow_dispatch`
发布**新的**签名 O→T 单 system 请求；若预置未通过，不发 live 请求。这是未来经单独授权的生产预检，不是本 Draft 已执行的动作。

固定 launcher 的 `consume` 阶段只接受批准的 request ID，并从上述已验暂存目录在等效 root
systemd 沙箱运行**新版 R poller**； `QINTOPIA_DEPLOY_RUNNER_BIN`
固定到 O 树的旧 runner，`COSCLI_PATH`
固定到已验现有工具。新版 poller 在实际下载、解析 COS
pointer 后、探测结果和下载请求之前比对
`QINTOPIA_EXPECTED_DEPLOY_REQUEST_ID`，下载请求后再核对解析出的 ID；错配即停止，绝不调用旧 runner。旧 O
runner 仍执行原 HMAC、scope、单目标、制品和 promotion 检查。标准旧 timer/service 全程受 hold 阻断，不能与此一次性 poller 竞争。预检和实际消费间的 pointer 竞态由**消费点**
比较解决，不能只用 launcher 预检代替。

新版 poller 在调用 runner 前取得独立
`poller.lock`，先检查是否存在任何未终结 claim，再以 request
ID、原签名请求摘要和 pointer 身份原子建
`/var/lib/qintopia-agent-os-deploy/requests/claimed/<id>.json`，fsync 文件和目录；跨进程死亡仍是“已可能执行”。

只有本地签名结果、COS 上传确认、请求归档及指针/结果核对都完成，才可终结该 claim。任何阶段中断，后续 T/R 普通 poller 先看到未终结 claim 并拒绝**所有**新消费；维护流程核对，不重放。
`poller.lock` 防并发，`deploy.lock`
防部署指针并发，持久 claim 防进程死亡后的重复执行。若结果上传成功而归档失败，仍保留 claim，不能仅凭远端对象存在自动清除。后续普通 R→T 回退同样由 T/R 新 poller 保护。

O→T 成功后的 T/O 指针、installer、单 system
smoke、签名 COS 回执与本地归档全相符，才可由固定 launcher 清除 hold/drop-in，恢复原 timer 状态。若任何一项不明，保持 hold 和 claim，走下文表列有限维护恢复。该前置暂存、launcher、poller
claim/expected ID 和 systemd 有效属性均尚未实现、模拟；在通过前首次 live 被阻断。

### 有限维护恢复的执行入口

在 R bundle 中提出固定
`deploy/runner/recover-release-lineage.sh`，由受审维护 runbook 在一次性 root systemd
service 中**固定到完整 T 目录**执行，不从 `current`
选择 poller/runner，也不接受任意可执行路径、目标 SHA 或服务名。入口先校验 T
bundle、当前/目标树校验表及已验签的原 O→T 或 R→T 请求；历史原请求可能过 TTL，仅在维护中验证归档 HMAC、key
id、原签名时间、production、四 SHA、scope、targets 与本地 manifest/持久事务，不把过期原请求当作新的 live 授权。正常回退仍由新请求完成。一次性 unit 的有效
`User`、 `Group`、`StateDirectory`、`WorkingDirectory`、`NoNewPrivileges`、
`PrivateTmp`、`ProtectHome`、`ProtectSystem`、`ReadWritePaths`
和固定 COS/state 环境须逐项等效于受审 root runner
unit，且不得授予岸岸 Profile 写权限；任何不等效即停。此入口及历史验签复用方式尚未实现、验证，属于上线阻断。

首次 O→T 的 hold 已在 live 前预置；后续回退进入不确定状态时，维护入口先 `disable --now`
普通 deploy timer，等待正在运行的普通 service **自然退出**
并确认无在途 poller；不得杀死未知事务。再由固定制品核对或持久写入
`/var/lib/qintopia-agent-os-deploy/recovery/` 下的 `hold` 与只读绑定记录，并核对受审固定
`qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf`：其
`ConditionPathExists=!/var/lib/qintopia-agent-os-deploy/recovery/hold`
使普通 service 即使被旧 timer 触发也不消费队列。`daemon-reload`
后检查有效 condition，重新检查 timer/service 均不活动，再取得与 runner **同一个**
`deploy.lock`。仅 `flock`
不能跨进程死亡保持排他；持久 hold、固定 drop-in、停用 timer 和 poller 未终结 claim 共同承接。

现有旧 poller 不持锁，且 runner 执行与 COS 回执上传、请求归档不原子。因此不能以 runner 锁替代上述隔离。

每次指针改动前，先在 root-owned
recovery 目录写原请求摘要、原指针、目标 manifest 摘要、允许状态、阶段与原 unit/timer 状态，使用临时文件、原子替换、文件及目录 fsync；同一
`deploy.lock` 下每次按实际指针和 manifest 做 CAS，写入后再记阶段。`rollback-from`
仅留证，不独自决定恢复方向。旧 O installer 恢复 service 文件后，drop-in 必须仍有效；再次
`daemon-reload` 并验证 condition、timer disabled、service
inactive。若 installer 覆盖或移除隔离，停止并保留 hold，不能启动旧 poller。若最终回到 O/P，因 O
runner 不认识岸岸，hold 与 timer 停用继续保留，直到另一次受审接管请求、COS
pointer/回执核对及独立授权解除。恢复到 T/O 或 R/T 后也只有在本地事务、COS 签名结果和归档状态一致、队列 pointer 安全时才可按原记录恢复 timer；不明则继续封闭。

### 有限状态恢复表

表中固定入口指上述**已验 T 树**内的 recovery
helper，通过一次性受限 systemd 单元执行；它支持岸岸但不自行从普通 COS
pointer 领新请求。若 T 目录尚不存在，仅允许只读核对，不伪造该入口。所有可写行均先完成持久 hold、普通 poller 退出、同一
`deploy.lock`、原签名请求与事务记录绑定；CAS 只接受表列的实际状态，不按参数猜测服务器现状。

| 实际状态与来源                                              | 固定入口、锁内动作                                                                                                                                                                                | 单元、结果与不确定性                                                                                                                                                      |
| ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| O/P；T 未落盘或首步尚未改指针                               | 只读核对 O/P、原请求、COS 结果和旧 runner；无 T 可执行入口，**不写指针**。停用旧 timer 并等普通 service 退出，结果不明则不继续发请求                                                              | 核对 O installer/服务状态；原请求不得重放。未有固定 T helper 时不能宣称已有中断恢复能力                                                                                   |
| O/O；旧 O promoter 已落完整 T，第一次写 `previous=O` 后中断 | 固定 T helper 验原 O→T 请求、T/O/P 树和 O manifest `previous_sha=P`；CAS `previous:O→P`，`current` 始终 O                                                                                         | 运行 O installer 与原请求单 system smoke；保留 hold 阻断旧 O poller，审计为有限维护恢复，不把 P 失败回执改写成成功                                                        |
| T/O；旧 O 首次过渡两指针已写，installer/smoke/回执未确认    | 固定 T helper 验原请求及 T manifest `previous_sha=O`；若没有已确认的成功签名回执，以既有 `rollback-release.sh --expected-current-sha T --expected-previous-sha O --restore-previous-sha P` 回 O/P | primitive 内可能经过 O/O；O installer 与**原单 system** smoke 必须完成。此请求未批准岸岸重启；不能要求 O smoke 执行 `hermes-anan`。若已有成功回执先核对、停止自动反向操作 |
| R/T；R→T 签名回退的第一处指针写前                           | 固定 T helper 核对本次新签名请求和持久意图，CAS 只确认 R/T，**不重发**回退                                                                                                                        | 检查此前排空影响，运行/核对 R installer 与获批目标 smoke；结果缺失则保持 hold，不能凭指针宣称成功                                                                         |
| T/T；R→T 的 `current=T` 已写、`previous` 尚为 T             | 固定 T helper 只凭 R/T 原事务记录和 T manifest `previous_sha=O`，CAS `current:T→R`，回到 R/T；不继续未知的正向动作                                                                                | R installer 与获批目标 smoke 成功才可记恢复；`rollback-from=R` 只留证。CAS 或单元失败保持 hold                                                                            |
| T/O；R→T 已写两指针，installer/smoke/回执未确认             | 根据**R→T 的持久事务**与首次 O→T 区分；若无可信成功回执，CAS `previous:O→T` 后 `current:T→R`，回 R/T；每步 fsync                                                                                  | 恢复 R installer 和获批目标 smoke；中途 T/T 仍按上一行处理。若签名成功回执已持久存在，只核对并归档，不改指针                                                              |
| 任一行的 installer、岸岸排空或 smoke 失败                   | 在原锁内且仅对仍匹配的有限状态尝试一次回原指针/旧单元；岸岸超时只取消本次 drain 并暂缓，不强杀、不补跑                                                                                            | 任一恢复步骤失败或状态漂移记 failed/unknown，hold 持续，不能循环补偿或把服务 active 当成功                                                                                |
| 指针/服务似已完成，但 COS 回执上传或本地归档丢失            | 固定 T helper 只读比对本地阶段记录、签名结果文件、COS 结果、pointer 与请求归档；没有唯一可信结论时不运行原请求                                                                                    | poller 的执行、上传、归档非原子；持久 hold 阻断当前或恢复后的旧 unit。独立记录核对结论和负责人动作，未知状态不得重放                                                      |

**T/O→O/P 只允许首次 O→T 的单 system 未完成事务**：原签名请求的 `restart_targets`
必须恰为
`[qintopia-system-services]`，其 journal/manifest 不得显示岸岸曾由此次事务重启。因此 O
installer 与 O smoke 足以覆盖此次授权的服务恢复；`rollback-release.sh`
即使删除 candidate-only 的新岸岸 unit，固定 T recovery helper 仍在不可变 T 树，hold
drop-in 仍在旧 service 上生效。恢复后旧 O
poller 保持隔离。不允许把同一入口用于 R 已成功后含岸岸目标的 T→O/P 第二跳；那是另一项未设计、未授权的变更。

正常 R/T→T/O 与失败补偿回 R/T 均使用 T/R 新兼容 runner、installer、岸岸固定 unit 和相应 smoke。验证真实服务恢复后才可称回退成功。

维护记录至少包含原请求 ID/签名摘要、固定 T/R/O/P
manifest 摘要、两指针各阶段、`rollback-from`、installer/smoke、drain 清理状态、systemd
unit 及 timer 有效属性、COS 回执/本地归档核对结果、执行人和审批来源。维护记录与普通部署签名回执分开：不能为未知原请求伪造
`succeeded`。每次中断后先读状态再决定是否继续；如果表外指针、未验制品、CAS 冲突、hold 失效或 O/P 旧 poller 可能抢消费，保持封闭并阻断 live。

### 业务恢复优先路径

T 带 R **完整** deploy bundle；该 bundle 的清单包括 `agents/anan`、
`skills/pms-operations`、`skills/person-foundation`、工作流、配置、runner 和 systemd 源。旧 Sidecar
runtime P + R
bundle 不是旧业务版本，R→T 首先恢复的是可执行的前次**部署树和 runner**，不能自动声称岸岸业务回退。

若需要恢复某些旧业务行为，优先在**新的受审提交**中恢复选定业务源码，保留兼容新 runner、沙箱修复和签名回退能力。

再用现有完整 bundle/Sidecar 构建与正常签名发布流程前进。不预设选择性 bundle 新格式，也不直接改 T 或 O 的不可变树。

逐文件核对所恢复源码与迁移、插件、配置、systemd 和现有数据的兼容性。以新的提交、制品和回执标识结果，不能把业务恢复归因于旧 SHA 的重用。

Green PMS `1.8.1` 的精确提交为
`2f6dc4bbe47622f91f278eeff9ae1b3d705f35fc`。并行只读核对该提交的
`packages/contracts/src/external-payments.ts`、`apps/api/src/external-payments.ts`、
`packages/db/src/integration-worker.ts` 发现：事件仍为 `pms.payments.v1`，
`sequence/headCursor` 为十进制字符串，head/feed 需要物业 READ 权限；固定推送
`/api/v1/ingress/pms/events` 用 `HMAC(POST\npath\nsentAt\ndeliveryId\nsha256(body))` 和
`X-QT-*` 头， `202 accepted`/`200 duplicate` 必须匹配 `event_id`、`receipt_id`；
`401/403` 持久暂停、`400/409/413/422` 进 dead
letter，其余、ACK 不符和未知传输用**同一事件**重试。这只是指定源码合同的观察，不是 Agent
OS 与 PMS
1.8.1 联合兼容通过；不能拿旧 PMS 版本测试充当 1.8.1 证据。后续需分别验证 R/T 和任何旧业务恢复候选对 1.8.1
API、权限、事件 schema、数据库迁移、原请求键结果查询、重复事件与未知 ACK 的行为。

不回滚实时 PMS 订单、资金、库存或 Agent
OS 事项、事件、游标、回执等业务数据；保留 Profile、凭据、会话、记忆、任务及通道状态。恢复前暂停岸岸相关新入口并核对在途结果，旧新消费者不得同时办理同一请求。本次 runner 接管不夹带七 Profile
Hermes 核心升级，也不并入 PR #723。

### 归档验签与真实 systemd 验证归属

维护 helper 需要核验**原已签名请求**，不需要 workflow 自动发现历史成功请求。复用 runner 现有
`validate_request()` 的 HMAC 规范化、key
id、environment、repository、四 SHA、scope/targets 检查，在**只读归档验签模式**
输出固定身份；仅此模式跳过“相对当前时钟仍未过期”的 live TTL 条件，仍核对签名时间与原
`created_at/expires_at`。该模式绝不进入
`run_promotion()`，不创建新请求或写指针。普通 live 路径的 TTL 和 HMAC 校验完全保留。
`recover-release-lineage.sh` 将归档模式输出与原请求文件摘要、T/R/O/P
manifest 和有限状态交叉核对。

结果一侧复用 `wait-deploy-result.sh` 当前对 HMAC、key id、时间、request
ID、environment 和完整元组的验证块，增加**给定本地 result 文件的只读离线模式**，不轮询 COS，也不推断缺失结果为失败；正常等待模式不变。维护 helper 只对从固定 COS
key 或 root-owned state 取到的结果调用此模式，检查二者一致。
`test-deploy-runner-promotion.mjs`
覆盖归档请求的篡改、错 key、错环境/身份、过期但原签名时有效及普通 live 仍拒过期；
`test-wait-deploy-result.mjs`
覆盖离线结果的签名、请求/元组错配、缺失及上传/归档间断。P 的 `failed`
回执只能作为失败事实，不能通过身份检查后自动升格为成功谱系。

真实 Linux 验证单独归属
`tools/deploy/test-deploy-runner-systemd-linux.mjs`，在**可销毁 Ubuntu 24.04 systemd
PID1 VM** 中执行
`sudo node tools/deploy/test-deploy-runner-systemd-linux.mjs`；测试仅用 VM 内模拟密钥、Profile、COS 和 release 根，实际启动 root
runner、固定岸岸 unit、一次性 takeover/recovery
unit，验证有效 mount 属性、marker 同目录临时写/ rename/清除、hold condition、旧 O
unit 复装、candidate-only 清理及进程死亡后 claim/指针状态。无真实服务、凭据或生产指针。该 fixture 在第二阶段实现，本阶段未运行；不新增 CI
job/step，也不以 macOS 替身测试代替。

### 文件范围与删减后果

下表是第一阶段**待批准的技术候选**，不是修改许可。已定位 **12 个现有实现/测试路径 +
5 个新固定制品/测试路径 = 17 个实现/测试文件**。实现时另须更新 `deploy/runner/README.md`
和 `docs/operations/production-deploy-runner.md` 两份操作合同，故第二阶段候选总数为
**19 个文件**；本 PR 只改本报告，不实施这 19 个文件。如隔离验证表明还要改真实 primitive 或其他合同，先修订数量并重走技术复审及具体 CI/部署审批，不以 19 为封顶。旧 13 不是上限或已批准范围。

| 分类       | 路径或合同                                                                                 | 删除或不改的实际缺口                                                                                           |
| ---------- | ------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------- |
| 必改 1     | `.github/workflows/rollback-production.yml`                                                | 无 T 混合四元组的 production 签名入口；仅补 `hermes-anan` 选项仍是双 tag/同 SHA 路径                           |
| 必改 2     | `deploy/runner/qintopia-agent-os-deploy-runner`                                            | 四 SHA 同值校验和普通 promoter 仍拒绝 T；没有专属锁内谱系/制品复验、journal 及反向失败补偿                     |
| 必改 3     | `deploy/runner/poll-deploy-requests.sh`                                                    | 首次 live 无消费点 expected ID 绑定；普通 R→T 在上传/归档中断后可能重领原请求；须加入 poller.lock 和持久 claim |
| 必改 4     | `deploy/runner/smoke-release.sh`                                                           | 岸岸仍在通用 runner mount namespace 中写 marker，`runuser` 无法绕过只读沙箱                                    |
| 必改 5     | `deploy/runner/install-release-systemd-units.sh`                                           | 固定岸岸一次性 unit 无受审安装/核验路径；不能手工写 `/etc/systemd/system`                                      |
| 必改 6     | `tools/deploy/build-deploy-bundle.mjs`                                                     | 新 launcher、unit、recovery helper、hold drop-in 不进入 R bundle 不可变清单                                    |
| 必改 7     | `tools/deploy/check-deploy-runner.mjs`                                                     | 缺混合/双 tag 互斥、目标集合、expected ID/claim、固定 unit 沙箱合同；属 CI 改动须单独批准                      |
| 必改 8     | `tools/deploy/test-deploy-runner-promotion.mjs`                                            | 无混合精确回退、原 manifest 与动作目标分离、同步失败及反向补偿 fixture；替身不能证明 primitive                 |
| 必改 9     | `tools/deploy/test-release-systemd-rollback.mjs`                                           | 无真实 `rollback-release.sh` 逐次指针写、fsync、installer 故障注入；primitive 即使源码不改也须验证新调用       |
| 必改 10    | `tools/deploy/test-deploy-runner-poller.mjs`                                               | 无首次固定消费 expected ID 错配、持久 claim、上传/归档丢失与原旧 poller 隔离覆盖                               |
| 必改 11    | `deploy/runner/wait-deploy-result.sh`                                                      | 维护无法复用现有结果 HMAC 合同离线核验已存在回执；正常等待模式保留，不增加历史自动发现                         |
| 必改 12    | `tools/deploy/test-wait-deploy-result.mjs`                                                 | 无离线回执篡改、错 request ID/元组、缺失及未知结果负例                                                         |
| 拟新增 1   | `deploy/runner/qintopia-agent-os-anan-drain-restart.service`                               | 无独立且短生命周期的最小 Profile 写权限执行入口                                                                |
| 拟新增 2   | `deploy/runner/recover-release-lineage.sh`                                                 | 首次单 system O/O、T/O 与反向 T/T、未知结果无固定 T 入口、CAS 与持久审计；SSH 手移指针不可替代                 |
| 拟新增 3   | `deploy/runner/run-fixed-takeover-request.sh`                                              | 首次 live 前无受审 R bundle 暂存、预置 hold、唯一 request ID 消费与退出后解除合同                              |
| 拟新增 4   | `deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf`            | O installer 恢复旧 service 后可能重新消费 COS pointer；`flock`、timer 停用或临时 mask 单独不足                 |
| 拟新增 5   | `tools/deploy/test-deploy-runner-systemd-linux.mjs`                                        | 无实际 systemd PID1 下的 mount/marker/hold/旧 unit 复装与中断验证；仅本地替身不足                              |
| 复用并实测 | `deploy/runner/rollback-release.sh`、现有 runner service、`runtime/hermes/restart_anan.py` | 稳定 R/T→T/O 及 T/O→O/P 原语、通用 runner 只读 Home、官方 drain 均保留；若实测要求改脚本，重审数目和权限       |
| 复用       | `deploy/runner/deploy-request.schema.json`、`tools/deploy/create-deploy-request.mjs`       | 现有四 SHA、rollback 字段和新请求签名可用；普通混合回退不需新增 recovery 对象                                  |
| 推迟       | workflow 历史请求/回执自动发现、永久通用 recovery 协议和日常 T→O 第二跳                    | 缩小为固定归档只读核验与有限状态，不把 P 失败回执当成功                                                        |

原 13 文件方案只作为**已替代历史**：它把普通混合回退、历史成功回执自动发现、新 recovery
schema/创建器、固定 poller expected
ID 和常驻通用恢复绑成一体，却漏了真实 primitive 故障测试及岸岸沙箱。此处保留双 tag 验证、生产签名、制品复验和排空，不把缩小范围等同降低门禁。

### CI 九项原则的技术结论

| 原则       | 本次判断                                                                                                                                                                                                                                                                                                                                     |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1 实际问题 | rollback workflow 仅收双 tag、四 SHA 固定同值，T 混合且未发布；现 runner/普通 promoter 也拒绝原目标集与本次动作目标不同                                                                                                                                                                                                                      |
| 2 数量     | 候选 1 个既有 workflow、0 个新 job/step、3 个既有步骤（resolve、制品核验、请求生成）；新增 5 个可选输入（T 四 SHA + 本次 CSV 目标），将 `release_tag` 由必填改为条件必填，补 `hermes-anan` 选项与 all 展开 2 处；0 个新依赖。1 个 checker 改 4 组合同（模式互斥、身份/目标、消费/claim、沙箱）；Linux 测试为独立 VM 手工命令，不新增 CI 门禁 |
| 3 依据     | v0.3.3 失败回执、O/P/T 缺失、现 workflow/runner/promoter/poller 代码、构造器+AJV 可表达而 runner 拒绝的隔离模拟                                                                                                                                                                                                                              |
| 4 复杂度   | 混合分支、持久 claim、有限维护比永久通用 recovery 有界；journal、hold、CAS 和岸岸独立 unit 仍是安全成本，不能删成裸脚本调用                                                                                                                                                                                                                  |
| 5 现有 CI  | 业务测试不能生成 production 签名混合请求；现 checker 不校验新模式互斥、消费点身份、持久 claim、权限及旧路径不放宽                                                                                                                                                                                                                            |
| 6 最小可用 | schema/创建器/等待器、部署 workflow 和双 tag 路径优先复用；只处理 exact previous、一次签名请求和表列有限维护状态                                                                                                                                                                                                                             |
| 7 无特例   | 不按岸岸或 T SHA 特赦；目标走允许集合，恢复靠 manifest、请求及有限状态，不在通用代码硬编码 O/P/T/R                                                                                                                                                                                                                                           |
| 8 部署相关 | workflow/checker 只涉及 rollback、制品和运行安全；PMS 业务代码、七 Profile 核心升级和 PR #723 不进入                                                                                                                                                                                                                                         |
| 9 后续复用 | exact-previous 混合身份、消费点固定 ID 和真实脚本故障测试可服务未来合法混合 release；恢复入口仍限于已审状态                                                                                                                                                                                                                                  |

上述是可独立复审的技术范围，CI workflow 与 checker 仍须依
`programming-agent-guardrails.md`
对具体改动获得明确批准，第二阶段才可编辑；当前评论和本 Draft 不构成 CI、部署或生产授权。

### 最小验证矩阵

| 关口                   | 必须验证的事实                                                                                                                                                                     | 当前状态                                                                                           |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| 原双 tag 与混合新输入  | 双 tag 的发布/非预发布/master 可达性、COS 制品及目标逻辑不变；混合四 SHA 与 T manifest 相符，模式互斥、签名篡改/非法 profile/scope/targets 在写指针前拒绝                          | 现构造器+AJV 能表达混合请求，runner 在四 SHA 同值处拒绝；新 workflow/runner 未实现                 |
| O→T 首次 live 前置     | 旧 O 仍为 O/P，R bundle 暂存清单/摘要有效；hold/drop-in 已生效且旧 timer/service 不消费；只有 pinned 新 poller 的 expected ID 才调用 O runner                                      | 旧 O 接受单 system 的隔离 fixture 已通过；预置和真实 systemd 未验证                                |
| poller 一次性          | pointer 被另一个合法请求覆盖、下载后换 ID、runner 前/后、COS 上传前/后、归档前/后中断；claim 未终结时无新执行，结果核对后才释放                                                    | 现 poller 无锁/claim，上传与归档非原子；新路径未实现                                               |
| R/T→T/O 普通回退       | T 原 manifest 的 O/P/R/T 元组、`previous_sha=O`、原目标与此次签名动作目标分别验证；T/O 树和 COS 三类制品通过；`--restore-previous-sha O` 后获批目标 smoke 与签名回执一致           | 现 runner/promoter 阻断；T 未部署、R 未生成，无可执行模拟                                          |
| 真实指针与单元故障     | 用真实 `rollback-release.sh` 注入 `rollback-from`、current、previous、fsync、installer、smoke 失败；验证 O/P、O/O、T/O、R/T、T/T 的 CAS 和一次补偿。O/P 恢复只认首次单 system 请求 | 当前真实 primitive 测试只覆盖旧合同；新故障矩阵未测。runner promotion 的 rollback 替身不算此项证据 |
| 岸岸排空沙箱           | 真实 Linux systemd 比对 runner/独立 anan unit 的有效 mount、用户、写路径；官方同目录 temp/rename/unlink 成功；通用 runner 拒写，已有 drain/600 秒超时/清理失败均暂缓               | 官方源码写路径已只读核对；独立 unit 未建，真实 Linux 未测                                          |
| 恢复后旧 poller 隔离   | O installer 和 candidate-only 清理后 T helper 仍可运行；hold condition 持续、timer/service 不消费；未知 COS 结果不重放                                                             | hold/launcher 尚未实现；不能把 `daemon-reload`、`runuser` 或 `--runtime mask` 当证明               |
| 归档验签               | 原请求只读模式验 HMAC 与原签名时序，live 仍拒过期；离线结果模式验签名、request ID、完整元组和状态，P `failed` 不作成功证据                                                         | 现有 runner/等待器验签逻辑可复用；离线模式和负例未实现                                             |
| Green PMS 1.8.1 与状态 | 对 R/T、业务前进恢复候选验证指定 1.8.1 API、READ 权限、schema/migration、原请求键、事件去重、202/200 ACK、持久暂停/dead letter/未知传输；不回滚业务数据及 Profile 状态             | 1.8.1 精确源码合同仅只读观察；两端兼容未验证，旧 PMS 版本测试不替代                                |

独立评审在 `12c7ee31` 基础上复跑当前
`test-release-systemd-rollback.mjs`、`test-deploy-runner-promotion.mjs`、
`test-deploy-runner-poller.mjs`、`test-wait-deploy-result.mjs`
均通过；它们只验证**现有**合同。新实现必须在真实 Linux
systemd 和真实 rollback 脚本上注入故障。本次文档阶段无需数据库，绝不连接标为历史污染证据的旧库或默认
`5432`。

### 阻断、复审和阶段边界

1. **第一阶段（本 Draft）**：仅收敛报告及具体技术候选，请独立复审。T 未部署、R 未生成；hold/claim/沙箱/真实回退均未模拟，不能称上线就绪。
2. **第二阶段（另开实现 PR）**：先经独立复审并对具体 CI/部署修改范围取得明确批准，再实施 19 文件候选及必要调整，运行真实 primitive、poller、systemd 和包验证；若增减文件/门禁，更新范围和批准记录。
3. **第三阶段（另行授权）**：核对生产最新 O/P、O 成功/P 失败签名回执、T/R 精确制品、预置 hold、COS
   pointer、排空及 rollback 证据；先 dry
   run 再分别批准 live，取得完整签名回执、指针、installer、获批目标 smoke 和业务验收。当前未授权部署、发布、合并或任何服务器写入。

任何校验失败、岸岸排空超时、旧 poller 隔离不成立、持久 claim 不可恢复、COS 回执不明、T/R 制品缺失或表外状态，都停在原地保留证据；不重放失败的 v0.3.3 请求，不手移指针，不以服务 active 替代用户可见成功。
