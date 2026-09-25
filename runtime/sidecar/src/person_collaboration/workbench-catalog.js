const kindNames = { agent: "智能体", group: "群", person: "人员", role: "岗位" };
function ledgerItems() {
  if (ledgerKind === "role")
    return state.organization.positions.map((p) => ({ ...p, scope: p.scope_id }));
  const records = state.organization.ledger.filter((x) => x.kind === ledgerKind),
    known =
      ledgerKind === "agent"
        ? state.agents.map((id) => ({ id, label: text(id) }))
        : ledgerKind === "person"
          ? state.people
          : state.groups;
  return [
    ...records,
    ...known
      .filter((p) => !records.some((x) => x.object_ref === p.id))
      .map((p) => ({
        id: p.id,
        object_ref: p.id,
        label: p.label,
        status: "active",
        description: "已有来源记录，尚未补充台账登记。",
        derived: true,
        verified: true,
      })),
  ];
}
function ledgerRelations(item) {
  return state.relations.filter(
    (r) =>
      ["active", "scheduled"].includes(r.status) &&
      (ledgerKind === "role"
        ? r.role === item.role_id && r.scope === item.scope_id
        : ledgerKind === "person"
          ? r.person === item.object_ref
          : ledgerKind === "agent"
            ? r.agent === item.object_ref
            : (audienceOf(r)?.groups || []).includes(item.object_ref))
  );
}
function renderLedger() {
  const root = $("ledger");
  root.replaceChildren();
  const create = button(
    "新增" + kindNames[ledgerKind],
    () => (ledgerKind === "role" ? openPosition() : openLedger()),
    "qo-primary"
  );
  create.disabled = !state.catalog_admin;
  root.append(
    titleRow("基础台账", "人员、智能体、群和岗位分别登记；关联与授权单独生效", create)
  );
  const types = el("div", undefined, "ql-types");
  types.setAttribute("aria-label", "台账类别");
  for (const [kind, name] of Object.entries(kindNames)) {
    const b = button(name, () => {
      ledgerKind = kind;
      ledgerSelection = null;
      ledgerSearch = "";
      discardPreview();
      renderLedger();
    });
    b.setAttribute("aria-pressed", String(ledgerKind === kind));
    types.append(b);
  }
  root.append(types);
  const editor = el("div", undefined, "editor-host");
  editor.id = "catalog-editor";
  root.append(editor);
  const layout = el("div", undefined, "ql-layout"),
    left = el("div"),
    right = el("div");
  right.id = "ledger-detail";
  const search = inputField(
    left,
    "ledger-search",
    "查找" + kindNames[ledgerKind],
    ledgerSearch,
    "search"
  );
  search.placeholder = "按名称、昵称或说明查找";
  const list = el("div", undefined, "ql-items");
  list.id = "ledger-items";
  list.setAttribute("aria-label", "台账记录");
  left.append(list);
  layout.append(left, right);
  root.append(layout);
  search.addEventListener("input", () => {
    ledgerSearch = search.value;
    drawList();
  });
  function drawList() {
    list.replaceChildren();
    const records = ledgerItems().filter((x) =>
      [x.label, x.nickname, x.description].join(" ").includes(ledgerSearch.trim())
    );
    let item =
      records.find((x) => x.id === ledgerSelection) ||
      (ledgerKind === "agent" && records.find((x) => x.object_ref === "erhua")) ||
      records[0];
    ledgerSelection = item?.id;
    for (const r of records) {
      const b = button("", () => {
        ledgerSelection = r.id;
        discardPreview();
        drawList();
      });
      b.className = "ql-item";
      b.setAttribute("aria-pressed", String(item?.id === r.id));
      const label = el("span");
      label.append(
        el("span", r.label + (r.nickname ? `（${r.nickname}）` : "")),
        el(
          "div",
          r.scope_id || r.scope
            ? labelOf("scopes", r.scope_id || r.scope)
            : ledgerKind === "person"
              ? "统一人员档案"
              : ledgerKind === "role"
                ? labelOf("scopes", r.scope_id)
                : "已登记能力 / 来源",
          "qo-sub"
        )
      );
      b.append(label, el("small", statusNames[r.status] || r.status));
      list.append(b);
    }
    if (!records.length) empty(list, "没有匹配记录，可清空搜索或新增登记。");
    renderLedgerDetail(item, right);
  }
  drawList();
  root.append(historyPanel());
}
function renderLedgerDetail(item, host) {
  host.replaceChildren();
  if (!item) {
    empty(host, "选择一条台账记录查看详情。");
    return;
  }
  const panel = box(item.label);
  panel.append(
    sub(`${statusNames[item.status]}${item.nickname ? ` · ${item.nickname}` : ""}`),
    sub(item.description || "尚无补充说明")
  );
  if (ledgerKind === "person")
    panel.append(
      el(
        "div",
        item.verified
          ? "已有核验来源；任职和住宿关系分别管理。"
          : "待核验：登记不等于确认自然人或渠道身份，当前不能任命。",
        "qo-note"
      )
    );
  if (ledgerKind === "agent")
    panel.append(
      el(
        "div",
        `${item.derived || item.verified ? "智能体已登记" : "仅台账登记"}；能力、合作配置和渠道启用情况见下方，分别核对。`,
        "qo-note"
      )
    );
  if (ledgerKind === "group")
    panel.append(
      el(
        "div",
        "群来自可访问的模拟清单；范围绑定与工作触达分别配置，不创建或解散外部群。",
        "qo-note"
      )
    );
  if (ledgerKind === "role")
    panel.append(
      pair(
        "上级岗位",
        state.organization.positions.find((x) => x.id === item.parent_id)?.label ||
          "无上级岗位"
      ),
      pair("岗位定义", labelOf("roles", item.role_id)),
      pair("工作范围", labelOf("scopes", item.scope_id))
    );
  const refs = ledgerRelations(item);
  panel.append(el("h4", ledgerKind === "person" ? "当前任职与协作" : "关联岗位"));
  if (!refs.length) panel.append(sub("当前没有有效关联。"));
  for (const r of refs) {
    const row = el("div", undefined, "ql-relation"),
      pos = state.organization.positions.find(
        (p) => p.role_id === r.role && p.scope_id === r.scope
      );
    row.append(
      el(
        "span",
        `${pos?.label || labelOf("roles", r.role)} · ${personName(r.person)} · ${agentName(r.agent)}${r.status === "scheduled" ? " · 尚未开始" : ""}`
      )
    );
    if (!r.immutable && r.can_manage) {
      if (pos) row.append(button("配置任职与范围", () => openWork(pos, r)));
      row.append(
        button(ledgerKind === "group" ? "解除群触达" : "解除本项协作", () => {
          if (ledgerKind === "group") {
            const { authority_grant, ...a } = audienceOf(r);
            a.groups = a.groups.filter((id) => id !== item.object_ref);
            preview(
              { kind: "set_audience", collaboration: r.id, audience: a },
              [
                ["解除触达", item.label],
                ["工作", `${personName(r.person)} · ${agentName(r.agent)}`],
                ["影响", "仅取消这项工作的此群触达，其他群和其他岗位保留。"],
              ],
              panel
            );
          } else
            preview(
              { kind: "end_collaboration", collaboration: r.id },
              [
                ["解除协作", `${personName(r.person)} · ${agentName(r.agent)}`],
                ["影响", "本项协作的权限立即失效，历史及其他任职保留。"],
              ],
              panel
            );
        })
      );
    }
    panel.append(row);
  }
  const controls = actions();
  const edit = button(item.derived ? "完善登记" : "编辑资料", () =>
    ledgerKind === "role" ? openPosition(item) : openLedger(item)
  );
  edit.disabled = !state.catalog_admin || item.status === "retired";
  controls.append(edit);
  if (!item.derived) {
    const operation = item.status === "retired" ? "restore" : "retire",
      label =
        operation === "restore"
          ? "恢复在册"
          : ledgerKind === "person"
            ? "归档人员"
            : ledgerKind === "group"
              ? "停用群触达"
              : "停用" + kindNames[ledgerKind];
    const lifecycle = button(
      label,
      () =>
        preview(
          {
            kind: "lifecycle",
            object: ledgerKind === "role" ? "position" : "ledger",
            id: item.id,
            operation,
          },
          [
            ["变更", label + "：" + item.label],
            ["关联工作", `${refs.length} 项当前协作`],
            [
              "影响",
              operation === "restore"
                ? "恢复台账，不恢复旧任职、授权、触达或任务。"
                : ledgerKind === "group"
                  ? "撤销本群范围绑定，停止本群触达；任职和训练等业务权限保留。恢复群后需重新绑定。"
                  : "立即结束对应关联，收回权限；历史保留，未完成事项另行交接。",
            ],
          ],
          panel
        ),
      "qo-danger"
    );
    lifecycle.disabled = !state.catalog_admin;
    controls.append(lifecycle);
    if (item.status === "draft" && !item.used) {
      const remove = button(
        "删除未使用草稿",
        () =>
          preview(
            {
              kind: "lifecycle",
              object: ledgerKind === "role" ? "position" : "ledger",
              id: item.id,
              operation: "delete",
            },
            [
              ["删除草稿", item.label],
              ["边界", "仅限从未使用的误建草稿；人员底层身份及审计仍保留。"],
            ],
            panel
          ),
        "qo-danger"
      );
      remove.disabled = !state.catalog_admin;
      controls.append(remove);
    }
  }
  panel.append(controls);
  if (ledgerKind === "role") {
    const definitions = details("岗位职责与工作范围定义");
    const role = state.roles.find((r) => r.id === item.role_id);
    definitions.append(
      sub(
        role?.description || "岗位定义可以复用到不同范围；修改定义不自动扩大已有权限。"
      ),
      chips((role?.duty_ids || []).map((id) => labelOf("duties", id))),
      actions(
        button("编辑岗位职责", () => openRole(role)),
        button("新增岗位定义", () => openRole()),
        button("维护职责定义", () => openDuties()),
        button("维护工作范围", () => openScopes())
      )
    );
    panel.append(definitions);
  }
  if (ledgerKind === "group")
    panel.append(
      details(
        "群与范围的关联",
        ...state.bindings
          .filter((b) => b.conversation === item.object_ref)
          .map((b) => sub(labelOf("scopes", b.scope))),
        button("维护范围绑定", () => openBindings(item))
      )
    );
  host.append(panel);
  if (ledgerKind === "person") host.append(identityPanel(item));
  if (ledgerKind === "agent") {
    const scope = refs[0]?.scope || selectedOrg()?.scope_id || state.scopes[0]?.id;
    host.append(readinessPanel(item.object_ref, scope));
  }
  if (ledgerKind === "role") host.append(constraintsPanel(item.scope_id));
}
function catalogForm(title) {
  discardPreview();
  $("catalog-editor").replaceChildren();
  const panel = box(title),
    form = el("form");
  form.id = "catalog-form";
  panel.append(form);
  $("catalog-editor").append(panel);
  return form;
}
function formSubmit(form, fn) {
  const submit = el("button", "预览变更", "qo-primary");
  submit.type = "submit";
  form.append(
    actions(
      submit,
      button("取消", () => {
        $("catalog-editor").replaceChildren();
        discardPreview();
      })
    )
  );
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    fn();
  });
  form.querySelector("input,select")?.focus();
}
function openLedger(item) {
  const kind = ledgerKind,
    form = catalogForm((item ? "编辑" : "新增") + kindNames[kind]);
  const sourceItems =
    kind === "person"
      ? personOptions()
      : kind === "agent"
        ? state.agents.map((id) => ({ id, label: agentName(id) }))
        : state.groups;
  if (item && !sourceItems.some((x) => x.id === item.object_ref))
    sourceItems.push({
      id: item.object_ref,
      label: item.label + " · 已有登记，待接入 / 核验",
    });
  const reference = selectField(
    form,
    "record-reference",
    kind === "person"
      ? "先查找已有人员"
      : kind === "group"
        ? "从已接入群清单选择"
        : "已登记的运行能力",
    sourceItems,
    item?.object_ref || "",
    kind === "person"
      ? "没有匹配，登记新的待核验人员"
      : kind === "agent"
        ? "登记新智能体（能力尚未接入）"
        : "请选择群"
  );
  reference.required = kind === "group";
  reference.disabled = !!item && !item.derived;
  const name = inputField(
    form,
    "record-label",
    kind === "person" ? "姓名或常用称呼" : "名称",
    item?.label || "",
    "text",
    true
  );
  const nickname = inputField(
    form,
    "record-nickname",
    "昵称（可选）",
    item?.nickname || ""
  );
  const description = inputField(
    form,
    "record-description",
    "说明 / 同名消歧依据",
    item?.description || "",
    "textarea",
    true
  );
  selectField(
    form,
    "record-scope",
    "归属范围（可选）",
    state.scopes.filter(active),
    item?.scope_id || "",
    "尚未指定"
  );
  selectField(
    form,
    "record-owner",
    "维护负责人（可选）",
    personOptions(),
    item?.owner_id || "",
    "尚未指定"
  );
  const status = selectField(
    form,
    "record-status",
    "登记状态",
    [
      { id: "active", label: "登记在册（不授予权限）" },
      { id: "draft", label: "仅保存草稿" },
    ],
    item?.status === "draft" ? "draft" : "active"
  );
  reference.addEventListener("change", () => {
    if (!name.value)
      name.value = sourceItems.find((x) => x.id === reference.value)?.label || "";
  });
  form.append(
    el(
      "div",
      kind === "person"
        ? "新增人员保持待核验，不能直接任命；同名不会自动合并。已有人员须从清单明确选择。"
        : kind === "agent"
          ? "登记不创建 runtime；接入能力与业务授权分别处理。"
          : "仅登记当前可访问群，不更改外部群成员。",
      "qo-note"
    )
  );
  formSubmit(form, () =>
    preview(
      {
        kind: "save_ledger",
        id: item && !item.derived ? item.id : null,
        object: kind,
        reference: reference.value || null,
        label: name.value,
        nickname: nickname.value,
        description: description.value,
        scope: $("record-scope").value || null,
        owner: $("record-owner").value || null,
        draft: status.value === "draft",
      },
      [
        ["台账", kindNames[kind]],
        ["名称", name.value],
        ["来源", reference.selectedOptions[0]?.textContent || "待接入"],
        ["登记状态", status.selectedOptions[0].textContent],
        ["生效边界", "登记不授予权限；停用后的旧任职不会恢复。"],
      ],
      form
    )
  );
}
function openPosition(item) {
  if (page !== "ledger") {
    ledgerKind = "role";
    navigate("ledger");
  }
  const form = catalogForm(item ? "编辑组织岗位" : "新增组织岗位");
  inputField(
    form,
    "position-label",
    "组织中显示的岗位名称",
    item?.label || "",
    "text",
    true
  );
  selectField(
    form,
    "position-role",
    "岗位职责定义",
    state.roles.filter(active),
    item?.role_id || "",
    "请选择岗位定义"
  ).required = true;
  selectField(
    form,
    "position-scope",
    "岗位工作范围",
    state.scopes.filter(active),
    item?.scope_id || "",
    "请选择范围"
  ).required = true;
  selectField(
    form,
    "position-parent",
    "上级岗位",
    state.organization.positions.filter((x) => active(x) && x.id !== item?.id),
    item?.parent_id || "",
    "无上级岗位"
  );
  inputField(
    form,
    "position-description",
    "责任说明",
    item?.description || "",
    "textarea",
    true
  );
  selectField(
    form,
    "position-status",
    "保存方式",
    [
      { id: "active", label: "启用岗位定义（不授予权限）" },
      { id: "draft", label: "仅保存草稿" },
    ],
    item?.status === "draft" ? "draft" : "active"
  );
  form.append(
    sub("有任职历史的岗位不可替换职责定义和范围；仍可修改名称、说明与上级归属。")
  );
  formSubmit(form, () =>
    preview(
      {
        kind: "save_position",
        id: item?.id || null,
        role: $("position-role").value,
        scope: $("position-scope").value,
        parent: $("position-parent").value || null,
        label: $("position-label").value,
        description: $("position-description").value,
        draft: $("position-status").value === "draft",
      },
      [
        ["岗位", $("position-label").value],
        ["上级", $("position-parent").selectedOptions[0].textContent],
        ["范围", $("position-scope").selectedOptions[0].textContent],
        ["权限", "上级关系与岗位登记不自动赋权。"],
      ],
      form
    )
  );
}
function openRole(role) {
  const form = catalogForm(role ? "编辑岗位职责定义" : "新增岗位职责定义");
  inputField(form, "role-label", "岗位定义名称", role?.label || "", "text", true);
  inputField(
    form,
    "role-description",
    "职责说明",
    role?.description || "",
    "textarea",
    true
  );
  checkList(form, "role-duties", state.duties.filter(active), role?.duty_ids);
  form.append(sub("移除正在使用的职责会被拒绝；新增职责不自动增加已有人员权限。"));
  formSubmit(form, () =>
    preview(
      {
        kind: "save_role",
        id: role?.id || null,
        label: $("role-label").value,
        description: $("role-description").value,
        duty_ids: selected("role-duties"),
      },
      [
        ["岗位定义", $("role-label").value],
        [
          "职责",
          selected("role-duties")
            .map((id) => labelOf("duties", id))
            .join("、") || "无",
        ],
      ],
      form
    )
  );
}
function openDuties(duty) {
  const form = catalogForm("维护职责定义");
  const choose = selectField(
    form,
    "duty-choice",
    "选择职责",
    state.duties,
    duty?.id || "",
    "新增职责"
  );
  choose.addEventListener("change", () =>
    openDuties(state.duties.find((d) => d.id === choose.value))
  );
  inputField(form, "duty-label", "职责名称", duty?.label || "", "text", true);
  inputField(
    form,
    "duty-description",
    "职责说明",
    duty?.description || "",
    "textarea",
    true
  );
  selectField(
    form,
    "duty-domain",
    "工作领域",
    state.domains.map((id) => ({ id, label: text(id) })),
    duty?.domain || state.domains[0]
  );
  checkList(
    form,
    "duty-actions",
    state.actions.map((id) => ({ id, label: text(id) })),
    duty?.available_actions
  );
  formSubmit(form, () =>
    preview(
      {
        kind: "save_duty",
        id: duty?.id || null,
        label: $("duty-label").value,
        description: $("duty-description").value,
        domain: $("duty-domain").value,
        available_actions: selected("duty-actions"),
      },
      [
        ["职责", $("duty-label").value],
        ["权限类别", selected("duty-actions").map(text).join("、")],
        ["边界", "定义可用类别，不自动授予权限。"],
      ],
      form
    )
  );
  if (duty)
    form.append(
      actions(
        button(active(duty) ? "停用职责" : "恢复职责", () =>
          preview(
            {
              kind: active(duty) ? "retire_catalog" : "restore_catalog",
              object: "duty",
              id: duty.id,
            },
            [
              ["职责", duty.label],
              ["影响", "有使用中的关联时拒绝停用。恢复定义不恢复旧权限。"],
            ],
            form
          )
        )
      )
    );
}
function openScopes(scope) {
  const form = catalogForm("维护工作范围");
  const choose = selectField(
    form,
    "scope-choice",
    "选择范围",
    state.scopes,
    scope?.id || "",
    "新增范围"
  );
  choose.addEventListener("change", () =>
    openScopes(state.scopes.find((s) => s.id === choose.value))
  );
  inputField(form, "scope-label", "范围名称", scope?.label || "", "text", true);
  if (!scope) {
    selectField(
      form,
      "scope-parent",
      "上级范围",
      state.scopes.filter(active),
      state.scopes.find((x) => !x.parent)?.id
    );
    selectField(
      form,
      "scope-kind",
      "范围类别",
      [
        { id: "building", label: "楼栋" },
        { id: "business", label: "业务范围" },
      ],
      "building"
    );
  }
  formSubmit(form, () =>
    preview(
      scope
        ? { kind: "update_scope", id: scope.id, label: $("scope-label").value }
        : {
            kind: "create_scope",
            parent: $("scope-parent").value,
            label: $("scope-label").value,
            scope_kind: $("scope-kind").value,
          },
      [
        ["工作范围", $("scope-label").value],
        ["边界", "范围定义不自动授予权限。"],
      ],
      form
    )
  );
  if (scope?.parent)
    form.append(
      actions(
        button(active(scope) ? "停用范围" : "恢复范围", () =>
          preview(
            {
              kind: active(scope) ? "retire_catalog" : "restore_catalog",
              object: "scope",
              id: scope.id,
            },
            [
              ["范围", scope.label],
              ["影响", "有任职、下级或群绑定时拒绝停用。"],
            ],
            form
          )
        )
      )
    );
}
function openBindings(item) {
  const form = catalogForm("维护群与范围绑定");
  const scope = selectField(
    form,
    "binding-scope",
    "工作范围",
    state.scopes.filter(active),
    state.bindings.find((b) => b.conversation === item.object_ref)?.scope ||
      state.scopes[0]?.id
  );
  const host = el("div");
  form.append(host);
  const draw = () => {
    host.replaceChildren();
    checkList(
      host,
      "binding-groups",
      state.groups,
      state.bindings.filter((b) => b.scope === scope.value).map((b) => b.conversation)
    );
  };
  scope.addEventListener("change", draw);
  draw();
  form.append(
    sub("这里维护该范围全部群绑定。移除绑定后，依赖它的触达在执行查询时立即失效。")
  );
  formSubmit(form, () =>
    preview(
      {
        kind: "set_groups",
        scope: scope.value,
        conversations: selected("binding-groups"),
      },
      [
        ["范围", labelOf("scopes", scope.value)],
        [
          "绑定的群",
          selected("binding-groups")
            .map((x) => labelOf("groups", x))
            .join("、") || "无",
        ],
        ["影响", "仅更新此范围的群绑定；不解散外部群。"],
      ],
      form
    )
  );
}
