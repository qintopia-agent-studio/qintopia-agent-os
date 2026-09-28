# 管理工作台受限锁能力

日期：2026-09-28。Owner：Agent OS / Sidecar。风险：高（身份、授权与并发）。

正式工作台沿用原 tenant 事务、密码 Person 会话和现有来源核验。其受限数据库角色对
`persons` 与 `person_identity_gateways` 等身份表只有读取权，不能直接执行现有 `FOR SHARE`
或 Gateway registry `SHARE`
表锁。锁不可删除：来源或 Gateway 变更时必须在保存前固定已核验的行；旧来源 writer 不一定先锁 collaboration
tenant，namespace 新行需要原表锁防幻影。

本迁移只增加五个固定 `SECURITY DEFINER` 锁函数，全部
`VOLATILE`，`search_path=pg_catalog,pg_temp`，表名全限定，无动态 SQL、可选表名或任意谓词。`management_ui_lock_gateway_registry()`
原样取得 Gateway `SHARE` 表锁，返回 `void`。另外四个函数分别固定非 Gateway
Person、身份候选、工作账号来源、工作账号操作授权的现有行锁谓词，完整执行查询并以
`ROW_COUNT > 0` 返回
`boolean`。调用者在同一事务内对 false 立即按原错误拒绝；true 后继续读取并核对同一实体的 Person、namespace、Gateway、范围、版本、宿主观测和当前管理 grant。身份候选可匹配多个 Gateway，函数须锁全部匹配行，不能用
`EXISTS` 或 `LIMIT 1`。

普通 SQLx 迁移创建函数并在同一事务撤销
`PUBLIC EXECUTE`，不创建数据库角色或假定某一 PostgreSQL 主版本。正式 UI 未获 EXECUTE 时保持关闭。启用前由部署 owner 以受审管理员身份准备专用
`NOLOGIN`、非 superuser、非 `CREATEROLE`、非 `BYPASSRLS`
函数 owner，授予它仅执行固定锁所需的表权限，再转移函数 owner；保留原迁移/runtime 身份的 EXECUTE，仅给
`qintopia_management_ui` 新增 EXECUTE。部署 owner 已只读核实生产应用数据库为 PostgreSQL
18.4；Gateway 表锁的专用 owner 可授 `MAINTAIN`，UI 不取得 `MAINTAIN`，且对
`persons`、`person_identity_gateways` 不获 UPDATE；`source_identity_links` 与
`work_accounts`
原获准的配置写权限保留。迁移本身不根据版本选择权限分支；启用预检核对实际版本、owner 和 ACL。

启用预检确认函数 owner、定义、ACL、`SECURITY DEFINER`、固定搜索路径和调用角色无 owner
membership、`SET ROLE`、schema
CREATE 或函数替换能力。专用隔离库仍须按真实受限角色和非空行验证 bootstrap、登录/state、身份预览保存、物业绑定、共享账号登记/停用、操作授撤权及并发 namespace 插入/版本变化。空表 SELECT 成功不代表锁能力可用。

回退先停管理路由和 UI 进程，保留函数、账号、会话、授权及审计；不在运行中删除锁函数或回放结果不明的保存命令。此迁移不改数据行、外部发送、CI 或部署配置。
