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

function businessPagedRows(host, scope, kind, label, renderItem) {
  const search = inputField(
    host,
    `business-${kind}-search`,
    `查找${label}`,
    "",
    "search"
  );
  const results = el("div");
  const feedback = sub("");
  let cursors = [null];
  let nextCursor = null;
  let requestNumber = 0;
  let appliedSearch = "";
  const previous = button("上一页", () => {
    cursors.pop();
    load();
  });
  const next = button("下一页", () => {
    cursors.push(nextCursor);
    load();
  });
  const searchButton = button("查找", () => {
    appliedSearch = search.value.trim();
    cursors = [null];
    load();
  });
  search.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      searchButton.click();
    }
  });
  async function load() {
    const request = ++requestNumber;
    const query = new URLSearchParams({
      scope,
      kind,
      search: appliedSearch,
      limit: "50",
    });
    if (cursors.at(-1)) query.set("after", cursors.at(-1));
    results.replaceChildren();
    feedback.textContent = "正在读取当前可用候选……";
    previous.disabled = true;
    next.disabled = true;
    try {
      const data = await api(`/api/business/candidates?${query}`);
      if (!host.isConnected || request !== requestNumber) return;
      const items = data[kind] || [];
      for (const item of items) results.append(renderItem(item));
      if (!items.length) results.append(sub(`没有匹配的${label}。`));
      nextCursor = data.next || null;
      previous.disabled = cursors.length === 1;
      next.disabled = !nextCursor;
      feedback.textContent = data.has_more
        ? `第 ${cursors.length} 页 · 还有候选`
        : `第 ${cursors.length} 页`;
    } catch (error) {
      if (!host.isConnected || request !== requestNumber) return;
      feedback.textContent = error.message;
      previous.disabled = cursors.length === 1;
    }
  }
  host.append(actions(searchButton), results, actions(previous, next), feedback);
  load();
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

function renderScopeCommunication(target, scope, current) {
  const panel = box("工作人员协作");
  const saved = current.communication || {};
  const contactLabel = (contact) => {
    const type = contact.subject_kind === "work_account" ? "共用工作账号" : "人员";
    return `${contact.subject_label} · ${type} · ${contact.channel_label} · ${contact.platform} · ${contact.gateway} · 来源 ${contact.channel_source_link_id.slice(0, 8)}`;
  };
  const selectedContacts = new Map();
  for (const contact of saved.contacts || []) {
    selectedContacts.set(contact.channel_source_link_id, {
      ...contact,
      label:
        contact.subject_label && contact.channel_label && contact.platform
          ? contactLabel(contact)
          : null,
    });
  }
  const savedGroup = saved.staff_group?.binding_id;
  let group =
    savedGroup && saved.staff_group.label
      ? {
          binding_id: savedGroup,
          label: saved.staff_group.label,
          platform: saved.staff_group.platform,
        }
      : null;
  const selected = el("div");
  const renderSelected = () => {
    selected.replaceChildren(el("h4", "已选群与渠道"));
    selected.append(
      sub(
        group?.label ||
          (savedGroup ? "原工作人员群需从当前候选重新核对" : "尚未选择工作人员群")
      )
    );
    for (const [key, contact] of selectedContacts) {
      const row = el("div", undefined, "qo-actions");
      row.append(
        el("span", contact.label || "原联系人渠道需从当前候选重新核对"),
        button("移除", () => {
          discardPreview();
          selectedContacts.delete(key);
          renderSelected();
        })
      );
      selected.append(row);
    }
  };
  function candidatePicker(kind, title) {
    const section = el("section");
    section.append(el("h4", title));
    const search = inputField(
      section,
      `business-${kind}-search`,
      `查找${title}`,
      "",
      "search"
    );
    const results = el("div");
    const feedback = sub("");
    let cursors = [null];
    let nextCursor = null;
    let requestNumber = 0;
    let appliedSearch = "";
    const previous = button("上一页", () => {
      cursors.pop();
      load();
    });
    const next = button("下一页", () => {
      cursors.push(nextCursor);
      load();
    });
    const searchButton = button("查找", () => {
      appliedSearch = search.value.trim();
      cursors = [null];
      load();
    });
    search.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        searchButton.click();
      }
    });
    async function load() {
      const request = ++requestNumber;
      results.replaceChildren();
      feedback.textContent = "正在读取当前可用候选……";
      previous.disabled = true;
      next.disabled = true;
      const query = new URLSearchParams({
        scope,
        kind,
        search: appliedSearch,
        limit: "50",
      });
      if (cursors.at(-1)) query.set("after", cursors.at(-1));
      try {
        const data = await api(`/api/business/candidates?${query}`);
        if (!panel.isConnected || request !== requestNumber) return;
        const items = kind === "groups" ? data.groups || [] : data.contacts || [];
        for (const item of items) {
          const row = el("div", undefined, "qo-scope-row");
          if (kind === "groups") {
            row.append(
              el("strong", item.label),
              sub(`${item.platform} · ${current.label} · 会话 ${item.conversation_id}`),
              button("选择群", () => {
                discardPreview();
                group = item;
                renderSelected();
              })
            );
            if (item.binding_id === group?.binding_id) group = item;
          } else {
            const key = item.channel_source_link_id;
            const type = item.subject_kind === "work_account" ? "共用工作账号" : "人员";
            const label = contactLabel(item);
            row.append(
              el("strong", item.subject_label),
              sub(
                `${type} · ${item.channel_label} · ${item.platform} · ${item.gateway} · 来源 ${item.channel_source_link_id.slice(0, 8)}`
              ),
              button("选择渠道", () => {
                discardPreview();
                selectedContacts.set(key, { ...item, label });
                renderSelected();
              })
            );
            const existing = selectedContacts.get(key);
            if (existing?.channel_source_link_id === item.channel_source_link_id)
              selectedContacts.set(key, { ...item, label });
          }
          results.append(row);
        }
        if (!items.length) results.append(sub("本页没有匹配的当前可用候选。"));
        nextCursor = data.next || null;
        previous.disabled = cursors.length === 1;
        next.disabled = !nextCursor;
        feedback.textContent = data.has_more
          ? `第 ${cursors.length} 页 · 还有候选`
          : `第 ${cursors.length} 页`;
        renderSelected();
      } catch (error) {
        if (!panel.isConnected || request !== requestNumber) return;
        feedback.textContent = error.message;
        previous.disabled = cursors.length === 1;
      }
    }
    section.append(actions(searchButton), results, actions(previous, next), feedback);
    return { section, load };
  }
  const groups = candidatePicker("groups", "工作人员群");
  const contacts = candidatePicker("contacts", "人员或工作账号及渠道");
  panel.append(
    selected,
    groups.section,
    contacts.section,
    actions(
      button("预览协作安排", () => {
        if (!group) return notice("请从当前候选选择工作人员群。", true);
        if ([...selectedContacts.values()].some((contact) => !contact.label))
          return notice("原联系人渠道仍需重新核对或移除。", true);
        if (selectedContacts.size > 20)
          return notice("一个工作范围最多选择 20 个联系人渠道。", true);
        previewBusiness(
          {
            kind: "set_scope_communication",
            scope,
            staff_group_binding_id: group.binding_id,
            contacts: [...selectedContacts.values()].map((contact) => ({
              subject_kind: contact.subject_kind,
              subject_id: contact.subject_id,
              channel_source_link_id: contact.channel_source_link_id,
            })),
          },
          [
            ["工作范围", current.label],
            ["工作人员群", group.label],
            [
              "联系渠道",
              [...selectedContacts.values()]
                .map((contact) => contact.label)
                .join("；") || "未选择",
            ],
          ],
          panel
        );
      })
    )
  );
  renderSelected();
  target.append(panel);
  groups.load();
  contacts.load();
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
  if (data.manageable_scopes_truncated)
    target.append(sub("管理范围摘要已达上限；当前仅显示已加载的范围。"));
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
  const canManageOperation = (operation) => {
    const action = data.operations[operation]?.action;
    return action === "read_business"
      ? current.read
      : action === "execute_business" && current.execute;
  };

  const bindings = box("物业范围");
  if (data.bindings_truncated)
    bindings.append(sub("物业摘要已达上限；下方仅显示已加载的物业绑定。"));
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

  renderScopeCommunication(target, scope, current);

  const observed = box("已观测、待登记工作账号");
  businessPagedRows(
    observed,
    scope,
    "observed_accounts",
    "待登记工作账号",
    (candidate) => {
      const row = el("div", undefined, "qo-scope-row");
      row.append(
        el("strong", candidate.label),
        sub(
          `共用工作账号 · ${candidate.source_type} · ${current.label} · ${candidate.gateway} · 来源 ${candidate.source_link.slice(0, 8)} · 已观测、待登记`
        )
      );
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
              ["范围", current.label],
              ["来源", `${candidate.source_type} · ${candidate.gateway}`],
            ],
            row
          )
        )
      );
      return row;
    }
  );
  target.append(observed);

  const accounts = box("账号与操作");
  businessPagedRows(accounts, scope, "accounts", "已登记工作账号", (account) => {
    const row = el("div", undefined, "qo-scope-row");
    row.append(
      el("h4", account.label),
      sub(
        `共用工作账号 · ${account.source_type} · ${current.label} · ${account.gateway} · 来源 ${account.source_link.slice(0, 8)} · ${account.active ? (account.current ? "在用" : "待核对") : "已停用"}`
      )
    );
    const granted = account.grants || [];
    for (const grant of granted) {
      const line = el("div", undefined, "qo-actions");
      line.append(
        el(
          "span",
          `${grant.property} · ${businessRoles[grant.role] || grant.role} · ${businessOperationLabel(grant.operation, data.operations[grant.operation] || {})}${grant.valid_until ? ` · 至 ${new Date(grant.valid_until).toLocaleString("zh-CN")}` : ""}`
        )
      );
      if (canManageOperation(grant.operation))
        line.append(
          button("撤权", () =>
            previewBusiness(
              { kind: "revoke_operation", grant: grant.id },
              [
                ["账号", account.label],
                ["物业", grant.property],
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
    if (account.grants_has_more)
      row.append(sub("此账号授权超过显示上限；请按物业和操作核对后再办理。"));
    if (account.active && account.current) {
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
    }
    if (account.active && account.can_disable)
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
    return row;
  });
  target.append(accounts);
}
