# Foundation broker 提前断连可靠性核对

基线f5d5696。岸岸本地联合fixture前两次Unix探针connect后直接drop，后续Python分别报stay_contacts_incomplete与ConnectionRefusedError；改为完整错误响应探针后通过，不能据此认定服务健壮。原失败日志由岸岸保留，本线不修改其服务/数据，也不重跑旧支付环境。

## 本地复现与最小处理

在当前隔离树使用真实Unix
socket，先单独观察accept后的peer_cred，再运行真实broker，由单线程执行顺序保证客户端在服务accept前关闭，捕获JoinHandle与底层io错误。再覆盖半截请求和不读回复即断连后，下一个正常协议请求仍得到正确认证/校验响应。独立子进程隔离测试环境变量；传输回归使用惰性连接池，不接触数据库。

只有确认单个连接的错误传播导致listener退出后才修服务；凭据不可读取或uid不匹配时仍拒绝该连接，不能跳过peer检查。启动、监听器和权限配置失败保持错误，不新增重试协议、并行连接机制、权限、CI、allowlist、迁移或生产操作。

## 验收

记录平台及原退出错误，修复前用例应明确失败；修复后同用例证明后续请求可处理，现有PG宿主认证测试验证模型token/错profile/错gateway仍拒绝、授权宿主正常受理。执行必要格式、严格Clippy和原工程入口，真实结果写入索引报告并提交冻结SHA。
