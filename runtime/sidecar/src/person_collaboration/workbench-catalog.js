let editingRole = null,
  editingDuty = null,
  editingScope = null;
function focusEditor(id) {
  discardPreview();
  $(id).hidden = false;
  window.scrollTo({ top: $(id).getBoundingClientRect().top + window.scrollY - 20 });
  $(id).querySelector("input")?.focus({ preventScroll: true });
}
function closeEditors() {
  for (const id of ["roleEditor", "dutyEditor", "scopeEditor", "groupEditor"])
    $(id).hidden = true;
  editingRole = null;
  editingDuty = null;
  editingScope = null;
}
function openRole(role = null) {
  if (busy) return;
  editingRole = role;
  $("roleEditorHeading").textContent = role ? "编辑岗位 · " + role.label : "新增岗位";
  $("roleName").value = role?.label || "";
  $("roleDescription").value = role?.description || "";
  checks("roleDuties", state.duties.filter(active), role?.duty_ids || []);
  $("roleForm")
    .querySelectorAll("input, textarea, button[type=submit]")
    .forEach((n) => (n.disabled = !state.catalog_admin || (role && !active(role))));
  focusEditor("roleEditor");
  if (!state.catalog_admin)
    notice("可以查看岗位设置；当前操作者没有修改岗位目录的管理权。");
}
function openDuty(duty = null) {
  if (busy) return;
  editingDuty = duty;
  $("dutyEditorHeading").textContent = duty ? "编辑职责 · " + duty.label : "新增职责";
  $("dutyName").value = duty?.label || "";
  $("dutyDescription").value = duty?.description || "";
  $("dutyDomain").value = duty?.domain || state.domains[0];
  renderDutyActions(duty?.available_actions || []);
  $("dutyForm")
    .querySelectorAll("input, textarea, select, button[type=submit]")
    .forEach((n) => (n.disabled = !state.catalog_admin || (duty && !active(duty))));
  focusEditor("dutyEditor");
}
function openScope(scope = null) {
  if (busy) return;
  editingScope = scope;
  if (!scope) {
    const root = state.scopes.find((item) => active(item) && !item.parent);
    if (root) $("parent").value = root.id;
  }
  $("scopeEditorHeading").textContent = scope
    ? "编辑范围 · " + scope.label
    : "新增工作范围";
  $("scopeName").value = scope?.label || "";
  $("parentField").hidden = Boolean(scope);
  $("scopeKindField").hidden = Boolean(scope);
  focusEditor("scopeEditor");
}
function catalogImpact(object, item) {
  const key = object === "duty" ? "duty" : object;
  const connections = state.relations.filter(
    (r) => r[key] === item.id && r.status === "active"
  );
  const lines = connections.map((r) => "当前连接：" + relationSummary(r));
  if (object === "scope") {
    state.scopes
      .filter((s) => active(s) && s.parent === item.id)
      .forEach((s) => lines.push("下级范围：" + s.label));
    state.bindings
      .filter((b) => b.scope === item.id)
      .forEach((b) => lines.push("绑定群聊：" + labelOf("groups", b.conversation)));
  }
  return lines;
}
function renderDutyActions(chosen = selected("dutyActions")) {
  const actions = state.actions.filter(
    (a) => a !== "technical_support" || $("dutyDomain").value === "technical_support"
  );
  dutyChecks("dutyActions", actions, chosen);
}
function changeImpactHint(change, code) {
  let rows = [];
  if (code === "role_duties_in_use")
    rows = state.relations.filter(
      (r) =>
        r.status === "active" &&
        r.role === change.id &&
        r.duty &&
        !change.duty_ids.includes(r.duty)
    );
  if (code === "duty_actions_in_use")
    rows = state.relations.filter(
      (r) =>
        r.status === "active" &&
        r.duty === change.id &&
        state.grants.some(
          (g) =>
            g.collaboration === r.id &&
            g.revocable &&
            g.mode !== "denied" &&
            !change.available_actions.includes(g.action)
        )
    );
  if (code === "catalog_in_use" && change.kind === "save_duty")
    rows = state.relations.filter((r) => r.status === "active" && r.duty === change.id);
  if (code === "catalog_in_use" && change.kind === "retire_catalog")
    rows = state.relations.filter(
      (r) => r.status === "active" && r[change.object] === change.id
    );
  return rows.length
    ? " 当前关联：" +
        rows.slice(0, 6).map(relationSummary).join("；") +
        (rows.length > 6 ? `；共 ${rows.length} 条，可在协作总览筛选查看。` : "。")
    : "";
}
function retireCatalog(object, item) {
  const impact = catalogImpact(object, item);
  let summary = `停用${{ role: "岗位", duty: "职责", scope: "范围" }[object]}：${item.label}\n保留定义与历史。当前仍在使用时，请先结束或调整任职；恢复台账不恢复已结束的授权。`;
  if (object === "duty") {
    const roles = state.roles.filter((r) => r.duty_ids.includes(item.id));
    if (roles.length)
      summary +=
        "\n以下岗位将不再提供这项职责：" + roles.map((r) => r.label).join("、");
  }
  if (impact.length) {
    notice("当前仍在使用，需先处理关联：" + impact.join("；"), true);
    window.scrollTo({ top: 0 });
    return;
  }
  preview({ kind: "retire_catalog", object, id: item.id }, summary);
}
function catalogButtons(card, object, item, edit, add) {
  const bar = el("div", undefined, "buttons");
  bar.append(button(state.catalog_admin ? "编辑" : "查看设置", () => edit(item)));
  if (active(item)) {
    if (add) bar.append(button("建立连接", () => startRelation({ [object]: item.id })));
    if (state.catalog_admin)
      bar.append(button("停用", () => retireCatalog(object, item), "danger"));
  } else if (state.catalog_admin) {
    bar.append(
      button("恢复台账", () =>
        preview(
          { kind: "restore_catalog", object, id: item.id },
          `恢复 ${item.label} 的定义，不恢复任何旧任职和授权。`
        )
      )
    );
  }
  card.append(bar);
}
function renderCatalogs() {
  $("createRole").disabled = !state.catalog_admin;
  $("createDuty").disabled = !state.catalog_admin;
  directory("roleCatalog", state.roles, (card, role) => {
    card.append(
      el("span", active(role) ? "岗位" : "已停用", "tag"),
      el("p", role.description || "尚未补充岗位说明")
    );
    const duties = role.duty_ids
      .map((id) => state.duties.find((d) => d.id === id))
      .filter(Boolean);
    duties.forEach((d) =>
      card.append(
        button(
          d.label,
          () => {
            showPage("duties");
            openDuty(d);
          },
          "tag-button"
        )
      )
    );
    if (!duties.length)
      card.append(el("p", "尚未关联职责。编辑岗位后可选择已有职责。", "muted"));
    const count = state.relations.filter(
      (r) => r.role === role.id && r.status === "active"
    ).length;
    card.append(
      el("p", `${count} 条当前连接。调整岗位不会自动扩大这些连接的权限。`, "muted")
    );
    catalogButtons(card, "role", role, openRole, duties.length > 0);
  });
  directory("dutyCatalog", state.duties, (card, duty) => {
    card.append(
      el("span", active(duty) ? text(duty.domain) : "已停用", "tag"),
      el("p", duty.description)
    );
    const summary = el("details");
    summary.append(
      el("summary", `适用的权限类别 · ${duty.available_actions.length} 项`)
    );
    duty.available_actions.forEach((a) => summary.append(el("p", text(a), "muted")));
    if (!duty.available_actions.length)
      summary.append(el("p", "当前是职责草稿，尚未设置可选权限。", "muted"));
    card.append(summary);
    const roles = state.roles.filter((r) => active(r) && r.duty_ids.includes(duty.id));
    card.append(
      el(
        "p",
        "适用岗位：" + (roles.map((r) => r.label).join("、") || "尚未关联岗位"),
        "muted"
      )
    );
    catalogButtons(card, "duty", duty, openDuty, roles.length > 0);
  });
  directory("scopeCatalog", state.scopes, (card, scope) => {
    const root = !scope.parent;
    card.append(
      el(
        "span",
        active(scope) ? (root ? "秦托邦 · 顶层" : "工作范围") : "已停用",
        "tag"
      )
    );
    if (scope.parent)
      card.append(el("p", "归属：" + labelOf("scopes", scope.parent), "muted"));
    const groups = state.bindings
      .filter((b) => b.scope === scope.id)
      .map((b) => labelOf("groups", b.conversation));
    card.append(el("p", "群聊：" + (groups.join("、") || "尚未绑定")));
    const bar = el("div", undefined, "buttons");
    bar.append(button("查看连接", () => browse("filterScope", scope.id)));
    if (active(scope)) {
      bar.append(
        button("建立连接", () => startRelation({ scope: scope.id })),
        button("设置群聊", () => {
          $("groupScope").value = scope.id;
          groupsForScope();
        })
      );
      if (!root)
        bar.append(
          button("编辑名称", () => openScope(scope)),
          button("移除", () => retireCatalog("scope", scope), "danger")
        );
    }
    card.append(bar);
  });
}
function groupsForScope() {
  groups();
  focusEditor("groupEditor");
}
function setupCatalogEvents() {
  $("roleDescription").required = true;
  $("dutyDomain").addEventListener("change", () => renderDutyActions());
  $("createRole").addEventListener("click", () => openRole());
  $("createDuty").addEventListener("click", () => openDuty());
  $("createScope").addEventListener("click", () => openScope());
  document.querySelectorAll("[data-close-editor]").forEach((b) =>
    b.addEventListener("click", () => {
      $(b.dataset.closeEditor).hidden = true;
      discardPreview();
    })
  );
  $("roleForm").addEventListener("submit", (event) => {
    event.preventDefault();
    const change = {
      kind: "save_role",
      id: editingRole?.id || null,
      label: $("roleName").value,
      description: $("roleDescription").value,
      duty_ids: selected("roleDuties"),
    };
    preview(
      change,
      `${editingRole ? "编辑" : "新增"}岗位：${change.label}\n说明：${change.description || "未填写"}\n关联职责：${change.duty_ids.map((id) => labelOf("duties", id)).join("、") || "暂不关联"}\n已有连接的权限不会自动增加。若移除仍在使用的职责，将拒绝本次变更。`
    );
  });
  $("dutyForm").addEventListener("submit", (event) => {
    event.preventDefault();
    const change = {
      kind: "save_duty",
      id: editingDuty?.id || null,
      label: $("dutyName").value,
      description: $("dutyDescription").value,
      domain: $("dutyDomain").value,
      available_actions: selected("dutyActions"),
    };
    preview(
      change,
      `${editingDuty ? "编辑" : "新增"}职责：${change.label}\n工作领域：${text(change.domain)}\n内容与边界：${change.description}\n适用权限：${change.available_actions.map(text).join("、") || "暂无，保存为职责草稿"}\n这些是可选类别，不会自动给任何人授权。若移除仍在使用的权限，将拒绝本次变更。`
    );
  });
  $("scopeForm").addEventListener("submit", (event) => {
    event.preventDefault();
    const change = editingScope
      ? { kind: "update_scope", id: editingScope.id, label: $("scopeName").value }
      : {
          kind: "create_scope",
          parent: $("parent").value,
          label: $("scopeName").value,
          scope_kind: $("scopeKind").value,
        };
    preview(
      change,
      editingScope
        ? `范围更名：${editingScope.label} → ${change.label}\n已有连接和群聊归属保持不变。`
        : `新增范围：${change.label}\n归属：${labelOf("scopes", change.parent)}`
    );
  });
  $("groupBinding").addEventListener("submit", (event) => {
    event.preventDefault();
    preview(
      {
        kind: "set_groups",
        scope: $("groupScope").value,
        conversations: selected("groups"),
      },
      `调整 ${labelOf("scopes", $("groupScope").value)} 对应的群聊：\n${
        selected("groups")
          .map((id) => labelOf("groups", id))
          .join("\n") || "清空绑定"
      }\n这会改变该范围对应的群，不会自动改变任何人的职责与权限。`
    );
  });
}
