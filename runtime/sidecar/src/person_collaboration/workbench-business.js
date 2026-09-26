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

function renderBusiness() {
  const target = $("business");
  target.replaceChildren();
  const data = state.business;
  if (!data?.can_manage) return;
  target.append(titleRow("岸岸账号授权", "QinTopia · 物业业务范围"));

  const bindings = box("物业范围");
  for (const binding of data.bindings.filter((item) => item.active)) {
    const row = el("div", undefined, "qo-scope-row");
    row.append(
      el("strong", `${binding.property} · ${labelOf("scopes", binding.scope)}`),
      sub(binding.source)
    );
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
  const scope = selectField(
    bindingForm,
    "business-scope",
    "范围",
    state.scopes.filter(active).map((item) => ({ id: item.id, label: item.label })),
    "",
    "选择范围"
  );
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
      if (!scope.value || !source.value.trim() || !property.value.trim())
        return notice("请填写范围、来源实例和物业 ID。", true);
      previewBusiness(
        {
          kind: "create_binding",
          scope: scope.value,
          source: source.value.trim(),
          property: property.value.trim(),
        },
        [
          ["范围", scope.selectedOptions[0].textContent],
          ["物业", property.value.trim()],
        ],
        bindingForm
      );
    })
  );
  bindings.append(bindingForm);
  target.append(bindings);

  const observed = box("可信来源候选");
  if (!data.observed.length) observed.append(sub("当前没有待登记的企微工作账号。"));
  for (const candidate of data.observed) {
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
  if (!data.accounts.length) accounts.append(sub("尚未登记工作账号。"));
  for (const account of data.accounts) {
    const row = el("div", undefined, "qo-scope-row");
    row.append(
      el("h4", account.label),
      sub(`${labelOf("scopes", account.scope)} · ${account.active ? "在用" : "已停用"}`)
    );
    const granted = data.bindings
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
      const permittedBindings = data.bindings.filter(
        (b) => b.active && b.scope === account.scope
      );
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
        Object.entries(data.operations).map(([id, spec]) => ({
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
