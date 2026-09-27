const businessRoles = { operator: "小客服", admin: "业务管理员" };
const businessActions = { read_business: "查询", execute_business: "办理" };

function businessOperationLabel(key, spec) {
  const name = {
    "pms.read.availability": "可售房源",
    "pms.read.room_status": "房态",
    "pms.read.orders": "订单列表",
    "pms.read.order": "单笔订单",
    "pms.read.members": "会员列表",
    "pms.read.member": "单个会员",
    "pms.read.payments": "收款流水",
    "pms.read.reference_catalog": "业务目录",
    "pms.quote": "报价",
    "pms.reminder.snooze": "暂缓工作提醒",
    "pms.command.CREATE_ORDER": "订房",
    "pms.command.RECORD_COLLECTION": "登记收款",
    "pms.command.CHECK_IN": "入住",
    "pms.command.CHECK_OUT": "退房",
    "pms.command.RESCHEDULE_STAY": "改期",
    "pms.command.EXTEND_STAY": "续住",
    "pms.command.SHORTEN_STAY": "缩住",
    "pms.command.MOVE_UNIT": "换房",
    "pms.command.CANCEL_ORDER": "取消预订",
  }[key];
  return `${name || key} · ${businessActions[spec.action] || spec.action}`;
}

async function previewBusiness(change, details, host) {
  if (busy) return;
  discardPreview();
  setBusy(true);
  try {
    const command = {
      operation_id: crypto.randomUUID(),
      expected_version: state.business.version,
      change,
    };
    const checked = await api("/api/business/preview", command);
    if (checked.persisted !== false) throw new Error("预览结果异常，请重新读取配置。");
    pending = { command, savePath: "/api/business/save" };
    const panel = el("div", undefined, "qo-preview");
    panel.id = "preview";
    panel.tabIndex = -1;
    panel.append(el("h3", "变更预览"));
    const list = el("dl");
    details.forEach(([key, value]) => list.append(pair(key, value)));
    panel.append(
      list,
      actions(
        button("确认保存", savePending, "qo-primary"),
        button("取消", discardPreview)
      )
    );
    host.append(panel);
    panel.focus();
    notice("请核对当前账号、物业和具体操作。");
  } catch (error) {
    onError(error);
  } finally {
    setBusy(false);
  }
}

let businessReturnIndex = 0;
function businessEntry(scope, label) {
  const entry = button(label, () => openBusiness(scope));
  entry.dataset.businessScope = scope;
  return entry;
}

function focusBusinessReturn(scope) {
  const entries = [...document.querySelectorAll("[data-business-scope]")].filter(
    (entry) => entry.dataset.businessScope === scope
  );
  const target =
    entries[businessReturnIndex] || entries[0] || $("overview").querySelector("h2");
  if (target?.tagName === "H2") target.tabIndex = -1;
  target?.focus();
}

function closeBusiness() {
  const scope = businessScope;
  navigate("overview");
  focusBusinessReturn(scope);
}

function openBusiness(scope) {
  if (!state.business?.manageable_scopes?.some((item) => item.id === scope)) return;
  const entries = [...document.querySelectorAll("[data-business-scope]")].filter(
    (entry) => entry.dataset.businessScope === scope
  );
  businessReturnIndex = Math.max(0, entries.indexOf(document.activeElement));
  businessScope = scope;
  navigate("overview", { keepBusiness: true });
  $("business-heading")?.focus();
}

function renderBusiness(target, scope) {
  target.replaceChildren();
  const data = state.business;
  const current = data?.manageable_scopes?.find((item) => item.id === scope);
  if (!current) return;
  const heading = titleRow(
    "物业业务授权",
    current.label,
    button("返回组织关系", closeBusiness)
  );
  heading.querySelector("h2").id = "business-heading";
  heading.querySelector("h2").tabIndex = -1;
  target.setAttribute("aria-describedby", "business-heading");
  target.append(heading);
  if (data.manageable_scopes.length > 1) {
    const scopePicker = selectField(
      target,
      "business-managed-scope",
      "管理范围",
      data.manageable_scopes,
      scope
    );
    scopePicker.addEventListener("change", () => openBusiness(scopePicker.value));
  }
  const scopedBindings = data.bindings.filter((item) => item.scope === scope);
  const scopedAccounts = data.accounts.filter((item) => item.scope === scope);
  const scopedObserved = data.observed.filter((item) => item.scope === scope);
  const canManageOperation = (operation) => {
    const action = data.operations[operation]?.action;
    return action === "read_business"
      ? current.read
      : action === "execute_business" && current.execute;
  };

  const bindings = box("物业范围");
  for (const binding of scopedBindings.filter((item) => item.active)) {
    const row = el("div", undefined, "qo-scope-row");
    row.append(
      el("strong", `${binding.property} · ${labelOf("scopes", binding.scope)}`),
      sub(binding.source)
    );
    if (current.read && current.execute)
      row.append(
        button("停用物业接线", () =>
          previewBusiness(
            {
              kind: "disable_binding",
              binding: binding.id,
              expected_binding_version: binding.version,
            },
            [
              ["物业", binding.property],
              ["影响", "此物业的岸岸操作授权全部撤回"],
            ],
            row
          )
        )
      );
    bindings.append(row);
  }
  const bindingForm = el("form");
  bindingForm.id = "business-binding-form";
  const source = inputField(
    bindingForm,
    "business-source",
    "PMS 来源实例",
    "",
    "text",
    true
  );
  const property = inputField(
    bindingForm,
    "business-property",
    "物业 ID",
    "",
    "text",
    true
  );
  bindingForm.append(
    button("新增物业绑定", () => {
      if (!source.value.trim() || !property.value.trim())
        return notice("请填写来源实例和物业 ID。", true);
      previewBusiness(
        {
          kind: "create_binding",
          scope,
          source: source.value.trim(),
          property: property.value.trim(),
        },
        [
          ["范围", current.label],
          ["物业", property.value.trim()],
        ],
        bindingForm
      );
    })
  );
  bindings.append(bindingForm);
  target.append(bindings);

  const observed = box("可信来源候选");
  if (!scopedObserved.length) observed.append(sub("当前没有待登记的企微工作账号。"));
  for (const candidate of scopedObserved) {
    const row = el("div", undefined, "qo-scope-row");
    row.append(el("strong", candidate.label), sub(labelOf("scopes", candidate.scope)));
    const label = inputField(
      row,
      `account-label-${candidate.source_link}`,
      "工作账号名称",
      candidate.label,
      "text",
      true
    );
    row.append(
      button("登记工作账号", () =>
        previewBusiness(
          {
            kind: "register_account",
            source_link: candidate.source_link,
            gateway: candidate.gateway,
            label: label.value.trim(),
          },
          [
            ["账号", label.value.trim()],
            ["范围", labelOf("scopes", candidate.scope)],
          ],
          row
        )
      )
    );
    observed.append(row);
  }
  target.append(observed);

  const accounts = box("账号与操作");
  if (!scopedAccounts.length) accounts.append(sub("尚未登记工作账号。"));
  for (const account of scopedAccounts) {
    const row = el("div", undefined, "qo-scope-row");
    row.append(
      el("h4", account.label),
      sub(`${labelOf("scopes", account.scope)} · ${account.active ? "在用" : "已停用"}`)
    );
    const granted = scopedBindings
      .flatMap((b) => b.grants.map((g) => ({ ...g, binding: b })))
      .filter((g) => g.work_account === account.id && !g.revoked);
    for (const grant of granted) {
      const line = el("div", undefined, "qo-actions");
      line.append(
        el(
          "span",
          `${grant.binding.property} · ${businessRoles[grant.role] || grant.role} · ${businessOperationLabel(grant.operation, data.operations[grant.operation] || {})}${grant.valid_until ? ` · 至 ${new Date(grant.valid_until).toLocaleString("zh-CN")}` : ""}`
        )
      );
      if (canManageOperation(grant.operation))
        line.append(
          button("撤权", () =>
            previewBusiness(
              { kind: "revoke_operation", grant: grant.id },
              [
                ["账号", account.label],
                ["物业", grant.binding.property],
                [
                  "操作",
                  businessOperationLabel(
                    grant.operation,
                    data.operations[grant.operation] || {}
                  ),
                ],
              ],
              row
            )
          )
        );
      row.append(line);
    }
    if (account.active) {
      const form = el("form");
      const permittedBindings = scopedBindings.filter((b) => b.active);
      const binding = selectField(
        form,
        `grant-binding-${account.id}`,
        "物业",
        permittedBindings.map((b) => ({ id: b.id, label: b.property })),
        "",
        "选择物业"
      );
      const role = selectField(
        form,
        `grant-role-${account.id}`,
        "岗位业务角色",
        Object.entries(businessRoles).map(([id, label]) => ({ id, label })),
        "operator"
      );
      const operation = selectField(
        form,
        `grant-op-${account.id}`,
        "具体操作",
        Object.entries(data.operations)
          .filter(
            ([, spec]) => current[spec.action === "read_business" ? "read" : "execute"]
          )
          .map(([id, spec]) => ({
            id,
            label: businessOperationLabel(id, spec),
          })),
        "",
        "选择操作"
      );
      const until = inputField(
        form,
        `grant-until-${account.id}`,
        "有效至",
        "",
        "datetime-local"
      );
      form.append(
        button("授予操作", () => {
          if (!binding.value || !operation.value)
            return notice("请选择物业与具体操作。", true);
          const deadline = until.value ? new Date(until.value).toISOString() : null;
          previewBusiness(
            {
              kind: "grant_account_operation",
              binding: binding.value,
              account: account.id,
              role: role.value,
              operation: operation.value,
              valid_until: deadline,
            },
            [
              ["账号", account.label],
              ["物业", binding.selectedOptions[0].textContent],
              ["角色", businessRoles[role.value]],
              ["操作", operation.selectedOptions[0].textContent],
            ],
            form
          );
        })
      );
      row.append(form);
      if (granted.every((grant) => canManageOperation(grant.operation)))
        row.append(
          button("停用账号", () =>
            previewBusiness(
              {
                kind: "disable_account",
                account: account.id,
                expected_account_version: account.version,
              },
              [
                ["账号", account.label],
                ["影响", "撤回该账号的岸岸操作授权"],
              ],
              row
            )
          )
        );
    }
    accounts.append(row);
  }
  target.append(accounts);
}
