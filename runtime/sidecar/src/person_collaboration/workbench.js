/* The server owns identity, authorization, versioning and persistence. */
let state,
  busy = false,
  pending = null,
  page = "overview",
  selectedPosition = null,
  editing = null;
let ledgerKind = "agent",
  ledgerSelection = null,
  ledgerSearch = "";
const errors = {
  pms_refresh_required: "住宿信息需要可信来源刷新，当前不能继续发送。",
  current_target_membership_required: "尚未确认居民已加入这个目标群。",
  content_required: "还没有可用内容，请先完成制卡和文案准备。",
  welcome_rule_required: "请先在合作约定里安排本次欢迎方式。",
  case_not_ready: "入住资料或使用许可尚未齐备。",
  card_not_eligible: "当前入住资料尚不满足制卡条件。",
  artifact_input_stale: "入住资料已更新，原卡片需重新制作。",
  rule_authority_revoked: "这项设置的原授权已失效，请重新安排。",
  designated_reviewer_required: "只有当前指定的审核人可以批准。",
  executor_unavailable: "协作智能体暂未就绪，事项已保留。",
  designation_authority_required: "当前没有自主指定审批代理的权限，请联系负责人核对。",
  current_resident_required: "此人当前不在本栋有效在住名单中，请刷新名单后选择。",
  delegate_not_current_resident: "代理人的当前在住状态无法确认，请刷新来源或重新安排。",
  delegate_must_be_another_resident: "请选择另一位当前在住人员。",
  review_delegation_expired: "临时代理已到期，请重新安排或撤销代理后由本人审核。",
  review_delegation_authority_revoked: "原委托授权已变化，代理不能继续审批。",
  delegation_version_conflict: "代理安排已被更新，请刷新后再操作。",
  review_authority_required: "当前缺少有效内容审核权。",
  content_approval_required: "等待当前审核人批准这份具体内容。",
  upper_confirmation_required: "仍需上层指定确认人批准发布。",
  private_workspace_only: "人员与待审内容请在私聊中查看和办理。",
  request_account_changed_or_disabled:
    "提交或确认请求的账号已停用或重置，请重新登录后提交新请求。",
  knowledge_stopped: "约定已停止。请刷新后选择明确的重新启用操作。",
  scheduled_revision_changed: "这项未来安排已经变更或开始生效，请刷新后重新选择。",
  invalid_effective_interval: "截止时间须晚于开始时间，且不能已经到期。",
  invalid_knowledge_content:
    "请填写约定名称与正文（名称不超过 80 字，正文不超过 12 KB）。",
  text_rule_required: "这不是可直接编辑的文本约定，请使用它所属的业务入口。",
  rejected_by_reviewer: "指定确认人已拒绝，未修改约定。",
  cancelled_by_requester: "请求人已撤销，未修改约定。",
  authority_changed_or_revoked: "原授权已变化或撤回，请重新核对。",
  approval_authority_changed_or_revoked: "确认人的授权已变化，原批准不再有效。",
  designated_confirmation_required: "只有当前指定且授权有效的确认人可以处理。",
  review_not_pending: "请求已经处理，请刷新查看结果。",
  use_rule_lifecycle: "文本约定请使用停止整项或取消未来安排。",
  knowledge_version_conflict: "约定已被更新。草稿已保留，请重新读取并核对后再保存。",
  scope_access_denied: "当前账号没有修改这个范围的权限，请联系负责人核对授权。",
  foundation_disabled: "本地业务入口未启用，暂时无法保存。",
  command_already_processed_refresh_state:
    "该操作已处理，请重新读取当前状态核对；未重复保存。",
  configuration_version_conflict: "配置已被其他窗口更新。重新读取后，核对并再次预览。",
  idempotency_conflict: "这次保存编号已经用于其他内容，请重新预览。",
  management_denied: "当前操作者没有这项工作的管理权。请联系有权负责人。",
  scope_access_denied: "当前操作者无权查看此范围。",
  catalog_management_required: "当前操作者没有维护基础台账的管理权。",
  catalog_management_denied: "当前操作者没有维护基础台账的管理权。",
  reviewer_required: "请为需要确认的操作选择确认人；其他方式不填写确认人。",
  reviewer_not_authorized:
    "确认人须为另一位已核验人员，并在同职责、智能体和范围拥有对应自主权。",
  invalid_permissions: "请补全决定方式和对应确认人。",
  person_not_verified: "该人员尚未核验，不能任命。请先完成身份核验。",
  verified_person_required: "该人员尚未核验，不能任命。",
  catalog_not_active: "所选人员、智能体或岗位已停用或仍是草稿，请先核对台账。",
  group_outside_scope: "所选群不在当前工作范围，或已停用。请调整群选择。",
  group_not_verified: "请从可访问的已接入群清单选择。",
  position_has_children: "此岗位仍有下级，请先调整下级的上级岗位，再停用。",
  invalid_position_parent: "上级岗位必须有效，且不能形成循环。",
  position_history_dimensions_fixed:
    "岗位已有任职历史，岗位定义和范围不能替换。请新建岗位并交接。",
  only_unused_draft_deletable: "仅从未使用的草稿可以删除。已有历史的记录请停用或归档。",
  ledger_identity_immutable: "已关联身份不能替换，请另建记录并核验。",
  immutable_root: "顶层和初始化管理关系受保护，不能在此移除。",
  bootstrap_relation_cannot_be_rewritten: "这是受保护的初始化管理关系。",
  immutable_collaboration: "初始化管理关系不可修改。",
  collaboration_not_active: "原工作安排已结束或到期。请从组织关系选择当前安排。",
  identity_changed_or_revoked: "会话身份已变更或撤销，请重新核验。",
  local_session_required: "本地会话已失效，请重新打开页面。",
  invalid_label: "请补全名称与说明，并检查长度。",
  invalid_text: "请填写实际工作说明，长度不超过 2000 字。",
  invalid_audience: "请核对人员范围与信息可见性。",
  existing_term_differs:
    "此人有同岗位同范围的其他工作。请沿用相同任期，或先结束原任职再配置。",
  expired_term: "任期截止时间须晚于开始时间，且不能已经到期。",
  invalid_term: "请核对任期：截止时间必须晚于开始时间。",
  invalid_term_range: "请核对任期：截止时间必须晚于开始时间。",
  invalid_term_start: "开始时间不符合当前任职条件。新任职请选择现在或未来时间。",
  term_start_in_past: "开始时间已经过去。请改为保存后立即开始，或重新选择未来时间。",
  existing_term_start_immutable:
    "已有任职的开始时间保留历史，不能改写。请另行安排新任职。",
  identity_management_denied: "当前账号没有身份管理权，请由获授权的身份管理者核对。",
  identity_version_conflict: "这个账号的关联已变化。请重新读取，核对现状后再次预览。",
  identity_gateway_version_conflict: "来源登记已变化。请重新读取候选，核对其最新范围。",
  identity_operation_conflict: "这次核验编号已用于其他内容。请重新读取并预览。",
  identity_scope_unbound: "来源与工作范围的关联已失效，请由来源维护者先核对登记。",
  shared_account_not_person:
    "共享账号不能直接确认为某个人，请选择已有可信观测的个人账号。",
  shared_account_person_unknown:
    "共享账号不能直接确认为某个人，请选择已有可信观测的个人账号。",
  identity_source_evidence_required: "来源缺少可信观测，请先由来源接入方补齐记录。",
  identity_observation_required: "来源缺少可信观测，请先由来源接入方补齐记录。",
  identity_namespace_conflict:
    "来源登记存在冲突，请先由技术负责人核对，再重新选择账号。",
  gateway_not_active: "账号来源或所属范围已停用，请先由来源维护者核对。",
  revoke_conflicting_link_first:
    "该账号已经关联另一人。请先核对并撤销原关联，不能直接覆盖。",
  revoke_person_mismatch: "该账号目前对应的人员已变化。请重新读取后核对撤销对象。",
  person_outside_tenant: "当前人员不属于可核验范围，请重新选择已有人员。",
  proxy_requires_expiry: "临时代理须填写截止时间。",
  invalid_proxy_appointment: "临时代理须关联同岗位同范围的另一项有效任职。",
  duty_not_available_for_role: "岗位未关联所选职责，请在岗位台账维护职责。",
  duty_domain_mismatch: "职责所属领域已变化，请重新选择。",
  action_outside_duty: "权限类别超出当前职责。",
  delegation_exceeds_authority: "可交给别人的权限超出当前操作者管理范围。",
  invalid_delegation: "请补全可安排的智能体、工作领域和权限。",
  catalog_in_use: "仍有有效关联，请先处理关联再停用。",
  role_in_use: "岗位仍有任职或工作，请先处理。",
  duty_in_use: "职责仍有关联，请先处理关联。",
  role_duties_in_use: "正在使用的职责不能移除，请先处理对应工作。",
  duty_actions_in_use: "当前工作仍使用此权限类别，请先调整授权。",
  scope_in_use: "范围仍有关联，请先处理下级或工作安排。",
  scope_has_active_bindings: "范围仍有下级或群绑定，请先处理关联。",
  configuration_not_saved: "保存未通过检查。请核对是否重复建立工作安排及当前权限。",
};
function notice(message, error = false) {
  $("notice").textContent = message;
  $("notice").classList.toggle("qo-error", error);
}
function setBusy(value) {
  busy = value;
  $("workspace").inert = value;
  $("workspace").setAttribute("aria-busy", String(value));
  document
    .querySelectorAll(".qo-tabs button")
    .forEach((b) => (b.disabled = value || !state));
  $("reload-state").disabled = value;
}
async function api(path, body) {
  let response;
  try {
    response = await fetch(
      path,
      body
        ? {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
          }
        : { cache: "no-store" }
    );
  } catch {
    throw new Error(
      body
        ? "尚未取得服务回执。请先重新读取状态核对是否已保存，不要直接重复操作。"
        : "无法连接本地服务。请检查服务是否运行，再重新读取。"
    );
  }
  let data;
  try {
    data = await response.json();
  } catch {
    throw new Error("服务未返回可核对的结果，请重新读取配置确认状态。");
  }
  if (response.status === 401) {
    location.replace("/login");
    throw new Error("登录已失效");
  }
  if (!response.ok) {
    const e = new Error(
      errors[data.code] ||
        "未通过服务端检查，未保存。请修正输入或重新读取配置。" +
          (data.code ? `（${data.code}）` : "")
    );
    e.code = data.code;
    throw e;
  }
  return data;
}
function discardPreview() {
  pending = null;
  document.querySelectorAll(".qo-preview").forEach((e) => e.remove());
}
function onError(error) {
  notice(error.message, true);
  $("recovery").hidden = false;
}
async function readState() {
  const next = await api("/api/state");
  if (!next.organization || !Array.isArray(next.relations))
    throw new Error("配置响应不完整，请检查本地服务版本。");
  state = next;
  ontologyRequests.clear();
  updateWorkspaceNavigation();
  $("result").textContent = state.management_available
    ? `已读取配置版本 ${state.version} · 本地验收`
    : "已读取最新工作安排";
  $("recovery").hidden = true;
}
function availablePage(next) {
  return (
    next === "overview" ||
    (next === "settings" && state.management_available === true) ||
    (next === "ledger" && state.catalog_admin === true)
  );
}
function updateWorkspaceNavigation() {
  const manager = state.management_available === true;
  $("workspace-title").textContent = manager
    ? "秦托邦 · 组织与智能体"
    : "秦托邦 · 我的工作";
  document.title = $("workspace-title").textContent;
  $("tab-overview").textContent = manager
    ? state.catalog_admin
      ? "组织关系"
      : "我的管理范围"
    : "我的工作";
  const navigation = document.querySelector(".qo-tabs");
  navigation.hidden = !manager;
  navigation.setAttribute("aria-label", manager ? "工作管理" : "我的工作");
  $("overview").setAttribute("role", manager ? "tabpanel" : "region");
  if (manager) {
    $("overview").setAttribute("aria-labelledby", "tab-overview");
    $("overview").removeAttribute("aria-label");
  } else {
    $("overview").setAttribute("aria-label", "我的工作");
    $("overview").removeAttribute("aria-labelledby");
  }
  for (const item of document.querySelectorAll("[data-page]"))
    item.hidden = !availablePage(item.dataset.page);
  $("workspace-boundary").textContent = manager
    ? "登记不自动授予权限；解除关联不删除对象。"
    : "只处理你的工作范围；保存时会核验当前授权。";
  if (!availablePage(page)) {
    page = "overview";
    editing = null;
    discardPreview();
  }
  // Revocation must remove stale management forms, not merely hide the tab.
  for (const id of ["settings", "ledger"])
    if (!availablePage(id)) $(id).replaceChildren();
  for (const item of document.querySelectorAll("[data-page]")) {
    const selected = item.dataset.page === page;
    item.setAttribute("aria-selected", String(selected));
    item.tabIndex = selected ? 0 : -1;
  }
  for (const id of ["overview", "settings", "ledger"]) $(id).hidden = id !== page;
}
function navigate(next) {
  if (busy || !state) return;
  discardPreview();
  page = availablePage(next) ? next : "overview";
  document.querySelectorAll("[data-page]").forEach((b) => {
    const yes = b.dataset.page === page;
    b.setAttribute("aria-selected", String(yes));
    b.tabIndex = yes ? 0 : -1;
  });
  for (const id of ["overview", "settings", "ledger"]) $(id).hidden = id !== page;
  if (page === "overview") renderOrganization();
  if (page === "ledger") renderLedger();
  if (page === "settings") renderSettings();
  notice("");
}
async function preview(change, summary, host, onSaved) {
  if (busy) return;
  discardPreview();
  setBusy(true);
  notice("正在核对权限和变更影响……");
  try {
    const command = {
      operation_id: crypto.randomUUID(),
      expected_version: state.version,
      change,
    };
    const checked = await api("/api/preview", command);
    if (checked.persisted !== false)
      throw new Error("预览结果异常，请重新读取确认状态。");
    pending = { command, onSaved };
    const panel = el("div", undefined, "qo-preview");
    panel.id = "preview";
    panel.tabIndex = -1;
    panel.append(el("h3", "本次变更预览"));
    const dl = el("dl");
    summary.forEach(([k, v]) => dl.append(pair(k, v)));
    if (checked.change?.status)
      dl.append(
        pair(
          "服务端登记状态",
          statusNames[checked.change.status] || checked.change.status
        )
      );
    if (checked.change?.ended_connections !== undefined)
      dl.append(
        pair(
          "服务端影响",
          `结束 ${checked.change.ended_connections} 条工作连接；恢复授权 0 条`
        )
      );
    panel.append(
      dl,
      sub("尚未保存。保存时会再次核验当前身份、权限和配置版本。"),
      actions(
        button("确认保存", savePending, "qo-primary"),
        button("继续修改", () => {
          discardPreview();
          notice("已返回修改，预览未产生持久变更。");
        })
      )
    );
    for (const impact of Array.isArray(checked.change?.impact)
      ? checked.change.impact
      : [])
      panel.insertBefore(el("p", impact, "qo-note"), panel.lastElementChild);
    host.append(panel);
    notice("服务端预览通过，请核对变更内容。");
  } catch (e) {
    onError(e);
  } finally {
    setBusy(false);
    $("preview")?.focus({ preventScroll: false });
  }
}
async function savePending() {
  if (!pending || busy) return;
  const operation = pending;
  setBusy(true);
  let saved = false;
  try {
    const result = await api(operation.savePath || "/api/save", operation.command);
    if (
      operation.savePath
        ? result.saved !== true && result.replayed !== true
        : result.persisted !== true
    )
      throw new Error("服务端未确认持久保存，请核对状态后重试。");
    saved = true;
    discardPreview();
    await readState();
    operation.onSaved?.();
    if (page === "settings") {
      page = "overview";
      editing = null;
    }
    setBusy(false);
    navigate(page);
    notice(operation.successMessage || "本次变更已保存，页面已读取最新状态。");
  } catch (e) {
    if (saved) notice("变更已保存，但读取失败。请重新读取核对，避免重复新建。", true);
    else onError(e);
    $("recovery").hidden = false;
  } finally {
    setBusy(false);
  }
}
document.querySelectorAll("[data-page]").forEach((b) => {
  b.addEventListener("click", () => navigate(b.dataset.page));
  b.addEventListener("keydown", (e) => {
    const tabs = [...document.querySelectorAll("[data-page]")].filter(
      (item) => !item.hidden && !item.disabled
    );
    let i = tabs.indexOf(b);
    if (e.key === "ArrowRight") i = (i + 1) % tabs.length;
    else if (e.key === "ArrowLeft") i = (i + tabs.length - 1) % tabs.length;
    else if (e.key === "Home") i = 0;
    else if (e.key === "End") i = tabs.length - 1;
    else return;
    e.preventDefault();
    navigate(tabs[i].dataset.page);
    tabs[i].focus();
  });
});
$("reload-state").addEventListener("click", async () => {
  if (busy) return;
  setBusy(true);
  discardPreview();
  try {
    await readState();
    if (page === "overview") renderOrganization();
    if (page === "ledger") {
      if (!$("catalog-form")) renderLedger();
      else {
        const latest = ledgerItems().find((x) => x.id === ledgerSelection);
        renderLedgerDetail(latest, $("ledger-detail"));
        $("latest-record")?.remove();
        if (latest) {
          const note = el("div", undefined, "qo-note");
          note.id = "latest-record";
          note.append(
            el("strong", "最新服务端记录"),
            sub(latest.label),
            sub(latest.description)
          );
          $("catalog-form").prepend(note);
        }
      }
    }
    notice("已读取最新配置。未保存的输入仍保留，请核对最新安排后再次预览。");
  } catch (e) {
    onError(e);
  } finally {
    setBusy(false);
  }
});
document.addEventListener("input", (e) => {
  if (e.target.closest("form") && !busy) discardPreview();
});
document.addEventListener("change", (e) => {
  if (e.target.closest("form") && !busy) discardPreview();
});
setBusy(true);
readState()
  .then(() => {
    setBusy(false);
    navigate("overview");
  })
  .catch((e) => {
    setBusy(false);
    onError(e);
    empty($("overview"), "暂时无法读取你的工作。可重新读取，或请负责人检查访问权限。");
  });

$("sign-out").addEventListener("click", async () => {
  try {
    await api("/api/logout", {});
    location.replace("/login");
  } catch (e) {
    onError(e);
  }
});
