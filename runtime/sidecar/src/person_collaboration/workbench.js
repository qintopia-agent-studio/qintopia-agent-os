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
  expired_term: "任期截止时间须晚于现在。",
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
    throw new Error("无法连接本地服务。请检查服务是否运行，再重试原操作。");
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
  $("result").textContent = `配置版本 ${state.version} · Postgres 已连接`;
  $("recovery").hidden = true;
}
function navigate(next) {
  if (busy || !state) return;
  discardPreview();
  page = next;
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
    const result = await api("/api/save", operation.command);
    if (result.persisted !== true)
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
    notice("已保存到本地数据库。刷新或重启服务后仍可读取；真实智能体尚未接入。");
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
    const tabs = [...document.querySelectorAll("[data-page]")];
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
    empty(
      $("overview"),
      "暂时无法读取组织配置。可重新读取，或请有权负责人检查访问权限。"
    );
  });

$("sign-out").addEventListener("click", async () => {
  try {
    await api("/api/logout", {});
    location.replace("/login");
  } catch (e) {
    onError(e);
  }
});
