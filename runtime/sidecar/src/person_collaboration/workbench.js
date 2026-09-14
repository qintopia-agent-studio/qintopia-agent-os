const $ = (id) => document.getElementById(id);
const names = {
  default: "default agent",
  erhua: "二花",
  xiaoman: "小满",
  silaoshi: "四老师",
  wenyuange: "文渊阁",
  guanerye: "关二爷",
  huabaosi: "阿靓",
  confirm_knowledge: "确认可用于回答的知识",
  train: "指导智能体改进服务",
  change_rules: "决定职责范围内的运营规则",
  review: "审核准备发给他人的内容",
  publish: "允许智能体按约定主动发消息",
  designate: "指定业务确认人",
  identity: "核对人员与渠道账号的关联",
  manage: "安排合作人员并授予权限",
  technical_support: "技术支持",
  community_service: "居民服务",
  activity_operations: "活动运营",
  hospitality: "客房服务",
  organization: "组织协作",
};
const modeNames = {
  autonomous: "可自主决定",
  confirmation: "需指定人员确认",
  denied: "未授予",
};
let state,
  pending,
  editing = null,
  prefilledDuty = null,
  busy = false;
const text = (key) => names[key] || key;
const active = (item) => !item.status || item.status === "active";
const labelOf = (kind, id) =>
  state[kind].find((x) => x.id === id)?.label || "未找到记录";
const dutyLabel = (r) =>
  r.duty ? labelOf("duties", r.duty) : r.immutable ? "初始化管理关系" : "待关联职责";
const selected = (id) =>
  [...$(id).querySelectorAll("input:checked")].map((x) => x.value);
function notice(message, error = false) {
  $("notice").textContent = message;
  $("notice").classList.toggle("error", error);
}
function setBusy(value) {
  busy = value;
  document.querySelector("main").inert = value;
  document.querySelector("main").setAttribute("aria-busy", String(value));
}
async function api(path, body) {
  const response = await fetch(
    path,
    body
      ? {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        }
      : {}
  );
  const data = await response.json();
  if (!response.ok) {
    const errors = {
      position_has_children: "这个岗位还有下级岗位。请先编辑下级的归属，再停用。",
      invalid_position_parent: "上级岗位必须有效，且不能形成循环的上下级关系。",
      position_history_dimensions_fixed:
        "岗位已有任职历史，不能改成另一岗位或范围。请新建岗位并交接任职。",
      only_unused_draft_deletable:
        "只有从未使用的误建草稿可删除；其他记录请停用并保留历史。",
      ledger_identity_immutable:
        "登记所关联的人员或平台身份不能直接替换；请另建记录并核验。",
      person_not_verified: "这位人员尚未完成身份核验，或台账已停用。登记不能代替核验。",
      catalog_not_active: "所选人员、智能体或岗位已停用 / 仍是草稿，请先核对台账状态。",
      group_outside_scope: "所选群未绑定在这条连接的范围内，或已经停用。",
      group_not_verified: "只能登记当前可访问且已接入的群。",
      action_outside_role: "该权限不适用于这个岗位，请先检查岗位关联的职责。",
      duty_not_assigned_to_role: "此岗位尚未关联所选职责，请先调整岗位，或改选职责。",
      duty_outside_role: "此岗位尚未关联所选职责，请先调整岗位，或改选职责。",
      duty_not_available_for_role: "此岗位尚未关联所选职责，请先调整岗位，或改选职责。",
      action_outside_duty: "所选权限超出了职责的适用类别，请重新选择。",
      duty_domain_mismatch: "职责的工作领域已变化，请刷新后重新选择。",
      invalid_permissions: "请为每项权限选择决定方式，并补全需要的确认人。",
      reviewer_required: "需要指定人员确认的权限，必须选择确认人。",
      invalid_reviewer:
        "确认人需是另一位自然人，并在同一智能体、职责和范围拥有此项自主决定权。",
      reviewer_not_authorized:
        "所选确认人当前没有同一智能体、职责和范围内的这项自主决定权。请先配置其连接，或另选有权人员。",
      self_confirmation_not_allowed:
        "不能选择自己作为确认人，请选择有对应自主决定权的另一位人员。",
      self_confirmation_forbidden:
        "不能选择自己作为确认人，请选择有对应自主决定权的另一位人员。",
      reviewer_mode_mismatch:
        "只有“需指定人员确认”时可以选择确认人，其他方式请清空确认人。",
      catalog_in_use:
        "该对象仍被当前任职、职责或连接使用。请先调整这些关联，再移除或停用。",
      role_in_use: "岗位仍有当前任职或连接。请先调整人员安排，再移除岗位。",
      duty_in_use: "职责仍与岗位或当前连接关联。请先处理这些关联，再移除职责。",
      scope_in_use: "范围仍有当前任职、下级范围或群绑定。请先处理这些关联。",
      scope_has_active_bindings:
        "此范围仍有下级范围或群绑定，请先在工作范围页处理这些关联。",
      duty_actions_in_use:
        "要移除的权限仍在连接中使用。请先调整相关连接的权限，再缩减职责。",
      role_duties_in_use: "要移除的职责仍有当前连接。请先调整相关连接，再修改岗位。",
      catalog_reference_conflict:
        "当前有连接依赖这项设置。请先调整相关连接，系统不会静默改变已有权限。",
      immutable_collaboration: "这是初始化管理关系，当前页面不支持改写。",
      immutable_root: "秦托邦顶层与初始化管理关系不可在这里移除。",
      root_scope_fixed: "秦托邦是顶层范围，不能在这里改名或移除。",
      bootstrap_relation_cannot_be_rewritten:
        "这是初始化管理关系，当前页面不支持改写。",
      collaboration_not_active: "这条连接已结束或到期，请刷新页面核对当前安排。",
      configuration_version_conflict: "配置已被更新，请刷新页面后重新预览。",
      idempotency_conflict: "这次变更编号已用于其他内容，请重新预览。",
      management_denied: "当前操作者没有管理这项工作或权限的授权。",
      catalog_management_denied: "当前操作者没有维护岗位、职责目录的管理权。",
      catalog_management_required: "当前操作者没有维护岗位、职责目录的管理权。",
      identity_changed_or_revoked: "当前会话的人员身份已变更或撤销，请重新核验。",
      delegation_exceeds_authority: "可交给别人的权限超过了当前操作者的管理范围。",
      configuration_not_saved:
        "未保存。请检查是否已有相同连接，以及岗位、职责与范围是否有效。",
      existing_term_differs:
        "此人已有同岗位、同范围的任职，请沿用其任期，或先处理原任职。",
      proxy_requires_expiry: "临时代理需要填写截止时间。",
      expired_term: "截止时间必须晚于现在。",
      invalid_proxy_appointment: "代理对象需要是同岗位、同范围的另一项有效任职。",
      invalid_delegation: "请完整选择可安排的智能体、领域和可授予的权限类别。",
      local_session_required: "本地会话已失效，请刷新页面。",
      invalid_label: "请补全名称和说明，并检查长度，不要输入控制字符。",
      invalid_text:
        "说明可以分行填写，请输入实际内容，并保持在 2000 字以内；不接受全空白或非法控制字符。",
    };
    const error = new Error(
      errors[data.code] ||
        "设置未通过检查，未保存。请核对当前连接与目录依赖。检查代码：" +
          (data.code || "unknown")
    );
    error.code = data.code;
    throw error;
  }
  return data;
}
function optionList(id, items, empty) {
  const target = $(id),
    old = target.value;
  target.replaceChildren();
  if (empty) target.add(new Option(empty, ""));
  items.forEach((x) => target.add(new Option(x.label, x.id)));
  if ([...target.options].some((o) => o.value === old)) target.value = old;
}
function checks(id, items, chosen = []) {
  $(id).replaceChildren();
  items.forEach((x) => {
    const label = el("label"),
      input = el("input");
    input.type = "checkbox";
    input.value = x.id;
    input.checked = chosen.includes(x.id);
    label.append(input, document.createTextNode(" " + x.label));
    $(id).append(label);
  });
}
function people() {
  const q = $("personSearch").value.trim();
  optionList(
    "person",
    state.people.filter(
      (p) => active(p) && (p.label.includes(q) || p.display_name.includes(q))
    ),
    "请选择具体人员"
  );
}
function groups() {
  const bound = state.bindings
    .filter((b) => b.scope === $("groupScope").value)
    .map((b) => b.conversation);
  checks("groups", state.groups, bound);
  filterGroups();
}
function filterGroups() {
  const q = $("groupSearch").value.trim();
  $("groups")
    .querySelectorAll("label")
    .forEach((l) => (l.hidden = !l.textContent.includes(q)));
}
function discardPreview() {
  pending = null;
  $("review").hidden = true;
}
function resetEdit() {
  editing = null;
  prefilledDuty = null;
  discardPreview();
  $("assignment").reset();
  people();
  optionList("role", state.roles.filter(active), "请选择岗位");
  $("assignmentHeading").textContent = "建立连接";
  $("draftState").textContent = "尚未保存";
  for (const key of ["role", "agent", "scope"]) $(key).value = "";
  roleDuties();
  setupDelegation();
}
function startRelation(preset = {}) {
  if (busy || !state) return;
  resetEdit();
  prefilledDuty = preset.duty || null;
  for (const key of ["person", "role", "scope", "agent"])
    if (preset[key]) $(key).value = preset[key];
  if (preset.duty && !preset.role) {
    const roles = state.roles.filter(
      (r) => active(r) && r.duty_ids.includes(preset.duty)
    );
    if (roles.length === 1) $("role").value = roles[0].id;
    else if (roles.length > 1)
      notice("已带入职责。请从适用岗位中选择；其他对象仍需设置。");
    optionList("role", roles, "请选择岗位");
  }
  roleDuties(preset.duty);
  showPage("assignment");
}
function editRelation(relation) {
  if (busy || relation.immutable) return;
  resetEdit();
  editing = relation;
  for (const key of ["person", "role", "scope", "agent"]) $(key).value = relation[key];
  roleDuties(relation.duty);
  $("responsibility").value = relation.responsibility;
  $("proxy").value = relation.proxy_for || "";
  if (relation.valid_until) {
    const time = new Date(relation.valid_until);
    $("until").value = new Date(time.getTime() - time.getTimezoneOffset() * 60000)
      .toISOString()
      .slice(0, 16);
  }
  const grants = state.grants.filter(
    (g) => g.collaboration === relation.id && g.revocable
  );
  permissionControls(grants);
  setupDelegation(grants.find((g) => g.action === "manage")?.management_envelope);
  $("assignmentHeading").textContent = "调整这条连接";
  $("draftState").textContent = "调整尚未保存";
  showPage("assignment");
  notice(
    relation.duty
      ? "可调整人、岗位、职责、智能体、范围和权限。保存会保留变更历史。"
      : "这条旧连接尚未关联职责，请先选定职责，再确认各项权限。不会自动扩大原有授权。"
  );
}
function setupDelegation(envelope) {
  checks(
    "managedAgents",
    state.agents.map((id) => ({ id, label: text(id) })),
    envelope?.agents
  );
  checks(
    "managedDomains",
    state.domains.map((id) => ({ id, label: text(id) })),
    envelope?.domains
  );
  dutyChecks("managedActions", state.actions, envelope?.actions);
  $("depth").value = String(envelope?.depth || 0);
  updateDelegation();
}
async function refresh() {
  state = await api("/api/state");
  prepareOrganizationState();
  state.duties ||= [];
  people();
  optionList("role", state.roles.filter(active), "请选择岗位");
  optionList("scope", state.scopes.filter(active), "请选择工作范围");
  optionList("parent", state.scopes.filter(active));
  optionList("groupScope", state.scopes.filter(active));
  optionList(
    "agent",
    state.agents.map((id) => ({ id, label: text(id) })),
    "请选择智能体"
  );
  optionList(
    "dutyDomain",
    state.domains.map((id) => ({ id, label: text(id) }))
  );
  optionList(
    "proxy",
    [
      ...new Map(
        state.relations
          .filter((r) => r.status === "active")
          .map((r) => [
            r.appointment,
            {
              id: r.appointment,
              label: `${labelOf("people", r.person)} · ${labelOf("roles", r.role)} · ${labelOf("scopes", r.scope)}`,
            },
          ])
      ).values(),
    ],
    "不是临时代理"
  );
  groups();
  roleDuties();
  setupDelegation();
  setupViews();
  renderCatalogs();
  render();
  renderOrganization();
  notice("当前配置已读取。岗位、职责与连接可在本地保存；真实智能体尚未接入。");
}
async function preview(change, summary) {
  if (busy) return;
  discardPreview();
  setBusy(true);
  notice("正在检查这次变更……");
  try {
    const command = {
      operation_id: crypto.randomUUID(),
      expected_version: state.version,
      change,
    };
    const checked = await api("/api/preview", command);
    pending = command;
    $("reviewText").textContent =
      summary +
      (checked.change?.ended_connections !== undefined
        ? `\n服务端核对：将结束 ${checked.change.ended_connections} 条连接，恢复授权 0 条。`
        : "");
    $("review").hidden = false;
    notice("检查通过，尚未保存。请核对上方的变更内容。");
  } catch (error) {
    notice(error.message + changeImpactHint(change, error.code), true);
  } finally {
    setBusy(false);
    window.scrollTo({ top: 0 });
    if (pending) $("review").focus({ preventScroll: true });
  }
}
$("assignment").addEventListener("submit", (event) => {
  event.preventDefault();
  const duty = state.duties.find((d) => d.id === $("duty").value);
  if (!duty) return notice("请先为这条连接选择明确的职责。", true);
  const permissions = readPermissions();
  const management = permissions.find(
    (p) => p.action === "manage" && p.mode === "autonomous"
  );
  const change = {
    kind: "assign",
    collaboration: editing?.id || null,
    person: $("person").value,
    role: $("role").value,
    duty: duty.id,
    scope: $("scope").value,
    agent: $("agent").value,
    domain: duty.domain,
    responsibility: $("responsibility").value,
    actions: [],
    permissions,
    valid_until: $("until").value ? new Date($("until").value).toISOString() : null,
    proxy_for: $("proxy").value || null,
    delegation: management
      ? {
          agents: selected("managedAgents"),
          domains: selected("managedDomains"),
          actions: selected("managedActions"),
          depth: Number($("depth").value),
        }
      : null,
  };
  let summary = `${editing ? "调整连接" : "建立连接"}\n人：${labelOf("people", change.person)}\n岗位：${labelOf("roles", change.role)}\n职责：${duty.label}\n智能体：${text(change.agent)}\n范围：${labelOf("scopes", change.scope)}\n说明：${change.responsibility}\n\n决定权：\n${permissions.map((p) => text(p.action) + "：" + modeNames[p.mode] + (p.reviewer ? " · " + labelOf("people", p.reviewer) : "")).join("\n") || "没有授予决定权"}\n\n任期：${change.valid_until ? new Date(change.valid_until).toLocaleString() + " 截止" : "无固定截止时间"}`;
  if (change.proxy_for)
    summary += "\n临时代理：" + $("proxy").selectedOptions[0].textContent;
  if (change.delegation)
    summary += `\n可安排的智能体：${change.delegation.agents.map(text).join("、")}\n可安排的工作领域：${change.delegation.domains.map(text).join("、")}\n可授予的权限：${change.delegation.actions.map(text).join("、")}\n可继续安排的层数：${change.delegation.depth}`;
  if (editing) summary += "\n本次只调整这条连接，不会结束此人的其他连接。";
  preview(change, summary);
});
$("save").addEventListener("click", async () => {
  if (!pending || busy) return;
  setBusy(true);
  let saved = false;
  try {
    await api("/api/save", pending);
    saved = true;
    discardPreview();
    await refresh();
    resetEdit();
    closeEditors();
    $("organizationEditor")?.remove();
    showPage(currentPage === "assignment" ? "overview" : currentPage);
    notice("已保存。真实训练、知识写入和消息发送尚未接入。");
  } catch (error) {
    notice(
      saved
        ? "变更已保存，但未能重新读取。请刷新核对，不要重复新建。"
        : error.message + " 如需重试，将沿用同一次保存编号。",
      true
    );
  } finally {
    setBusy(false);
  }
});
$("cancel").addEventListener("click", () => {
  if (!busy) {
    discardPreview();
    notice("已返回修改，这次预览没有保存。");
  }
});
$("resetEdit").addEventListener("click", () => {
  if (!busy) resetEdit();
});
document.querySelectorAll("form").forEach((form) =>
  form.addEventListener("input", () => {
    if (busy) return;
    discardPreview();
    $("draftState").textContent = "有未保存的调整";
  })
);
$("search").addEventListener("input", render);
$("personSearch").addEventListener("input", () => {
  people();
  permissionControls();
});
$("role").addEventListener("change", () => roleDuties());
$("duty").addEventListener("change", () => {
  prefilledDuty = $("duty").value || null;
  permissionControls();
  dutyContext();
});
for (const id of ["person", "scope", "agent"])
  $(id).addEventListener("change", () => {
    permissionControls();
    scopeContext();
  });
$("groupScope").addEventListener("change", groups);
$("groupSearch").addEventListener("input", filterGroups);
$("permissions").addEventListener("change", updateDelegation);
document
  .querySelectorAll("[data-page]")
  .forEach((b) => b.addEventListener("click", () => showPage(b.dataset.page)));
$("createRelation").addEventListener("click", () => startRelation());
for (const id of [
  "filterPerson",
  "filterScope",
  "filterAgent",
  "filterDuty",
  "filterStatus",
])
  $(id).addEventListener("change", render);
$("graphView").addEventListener("click", () => chooseView(true));
$("listView").addEventListener("click", () => chooseView(false));
setupCatalogEvents();
refresh().catch((error) => {
  notice(error.message, true);
  if (!state) {
    document.querySelectorAll("[data-view]").forEach((view) => (view.hidden = true));
    document
      .querySelectorAll("nav button")
      .forEach((button) => (button.disabled = true));
  }
});
