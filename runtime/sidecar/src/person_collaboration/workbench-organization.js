function selectedOrg() {
  return (
    state.organization.positions.find((x) => x.id === selectedPosition) ||
    state.organization.positions.find((x) => x.label === "社区负责人") ||
    state.organization.positions[0]
  );
}
function renderOrganization() {
  const target = $("overview");
  target.replaceChildren();
  const layout = el("div", undefined, "qo-layout"),
    tree = el("aside", undefined, "qo-tree"),
    detail = el("div");
  tree.setAttribute("aria-label", "组织与岗位结构");
  tree.append(el("h3", "秦托邦"), sub("选择岗位，查看当前安排"));
  const pos = selectedOrg();
  selectedPosition = pos?.id;
  const seen = new Set();
  function branch(parent, into) {
    for (const p of state.organization.positions
      .filter(
        (x) =>
          (state.organization.positions.some((p) => p.id === x.parent_id)
            ? x.parent_id
            : null) === parent
      )
      .sort((a, b) => {
        const order = (x) =>
          ({ 一栋: 1, 二栋: 2, 三栋: 3 })[labelOf("scopes", x.scope_id)] || 10;
        return order(a) - order(b) || a.label.localeCompare(b.label, "zh-CN");
      })) {
      if (seen.has(p.id)) continue;
      seen.add(p.id);
      const rs = currentRelations(p),
        b = button("", () => {
          selectedPosition = p.id;
          editing = null;
          renderOrganization();
        });
      b.className = "qo-node";
      b.setAttribute("aria-pressed", String(p.id === selectedPosition));
      b.append(
        el("span", p.label),
        el(
          "small",
          active(p)
            ? [...new Set(rs.map((r) => personName(r.person)))].join("、") ||
                "空缺 · 待安排"
            : statusNames[p.status]
        )
      );
      into.append(b);
      const children = el("div", undefined, "qo-branch");
      branch(p.id, children);
      if (children.children.length) into.append(children);
    }
  }
  branch(null, tree);
  if (state.catalog_admin)
    tree.append(
      button("维护岗位结构", () => {
        ledgerKind = "role";
        ledgerSelection = pos?.id;
        navigate("ledger");
      })
    );
  layout.append(tree, detail);
  target.append(layout);
  if (!pos) {
    empty(
      detail,
      state.catalog_admin
        ? "尚无组织岗位。登记岗位不会自动授予权限。"
        : "当前没有可见的有效任职。账号开通不增加业务权限，请联系负责人配置。",
      ...(state.catalog_admin ? [button("新增岗位", () => openPosition())] : [])
    );
    return;
  }
  const rs = currentRelations(pos),
    parent = state.organization.positions.find((x) => x.id === pos.parent_id),
    people = [...new Set(rs.map((r) => personName(r.person)))];
  const edit = button(
    rs.length ? "配置这项任职" : "安排任职",
    () =>
      openWork(
        pos,
        rs.find((r) => !r.immutable)
      ),
    "qo-primary"
  );
  edit.disabled =
    !state.management_available ||
    !active(pos) ||
    (rs.length > 0 && rs.every((r) => r.immutable));
  detail.append(
    titleRow(
      `${people.join("、") || "暂缺任职人员"} · ${pos.label}`,
      `上级岗位：${parent?.label || (pos.parent_id ? "当前权限未展示上级岗位" : "无上级岗位")}`,
      edit
    )
  );
  const manage = box("管理关系"),
    dl = el("dl"),
    grants = state.grants.filter(
      (g) => rs.some((r) => r.id === g.collaboration) && g.effective
    ),
    management = grants.filter((g) => g.action === "manage" && g.mode === "autonomous"),
    managed = [
      ...new Set(management.flatMap((g) => g.management_envelope?.agents || [])),
    ];
  dl.append(
    pair("负责范围", labelOf("scopes", pos.scope_id)),
    pair(
      "有效管理权",
      management.length
        ? management
            .map(
              (g) =>
                `${personName(g.person)}：${g.management_envelope.domains.map(text).join("、")}；可安排 ${managed.map(agentName).join("、")}`
            )
            .join("；")
        : "此岗位没有有效组织管理授权；任职连线不自动赋权"
    ),
    pair(
      "协作智能体",
      [...new Set(rs.map((r) => agentName(r.agent)))].join("、") || "尚未安排"
    ),
    pair(
      "下级岗位",
      state.organization.positions
        .filter((p) => p.parent_id === pos.id && active(p))
        .map((p) => p.label)
        .join("、") || "无下级岗位"
    )
  );
  manage.append(dl);
  detail.append(manage);
  const contact = box("智能体在这项工作中的触达范围");
  if (!state.contact_configuration_visible)
    empty(contact, "联系对象与触达配置仅向有权管理人员展示。");
  else if (!rs.length) empty(contact, "任职后按工作配置群和个人范围。");
  for (const r of state.contact_configuration_visible ? rs : []) {
    const a = audienceOf(r),
      row = el("div", undefined, "qo-scope-row");
    row.append(
      el("h4", `${agentName(r.agent)} · ${labelOf("duties", r.duty)}`),
      sub("群"),
      chips(
        a?.groups.length
          ? a.groups.map((id) => labelOf("groups", id))
          : ["未配置群触达"]
      ),
      sub("个人／人员集合"),
      el(
        "div",
        a?.people.length ? a.people.map(personName).join("、") : "未选择具体人员"
      ),
      sub(audienceSummary(a))
    );
    if (a?.residents && a.residents !== "none")
      row.append(sub("动态名单待 PMS 身份与住宿解析；当前未执行联系。"));
    contact.append(row);
  }
  detail.append(contact);
  const powers = box("在以上范围内可以做什么");
  if (!rs.length) empty(powers, "空缺岗位没有可执行的业务权限。");
  for (const r of rs) {
    const row = el("div", undefined, "qo-scope-row");
    row.append(el("h4", `${personName(r.person)} · ${agentName(r.agent)}`));
    const gs = grants.filter((g) => g.collaboration === r.id);
    if (!gs.length) row.append(sub("未授予有效决定权"));
    for (const g of gs)
      row.append(
        el(
          "div",
          `${modeNames[g.mode]}：${text(g.action)}${g.reviewer ? ` · ${personName(g.reviewer)}` : ""}`
        )
      );
    row.append(sub(r.responsibility));
    if (!r.immutable && r.can_manage) {
      row.append(
        actions(
          button("调整 / 换人", () => openWork(pos, r)),
          button("解除这项协作", () =>
            preview(
              { kind: "end_collaboration", collaboration: r.id },
              [
                ["解除协作", `${personName(r.person)} · ${agentName(r.agent)}`],
                ["范围", labelOf("scopes", r.scope)],
                ["影响", "对应权限立即失效；此人的其他工作和其他楼栋保留。"],
              ],
              row
            )
          ),
          button("结束整项任职", () =>
            preview(
              { kind: "end_appointment", appointment: r.appointment },
              [
                ["结束任职", `${personName(r.person)} · ${pos.label}`],
                [
                  "影响",
                  `结束此任职的 ${state.relations.filter((x) => x.appointment === r.appointment && x.status === "active").length} 项协作，立即收回对应权限。历史保留，不等待交接。`,
                ],
              ],
              row
            )
          )
        )
      );
    }
    const check = details(
      "核对当前决定权",
      button("服务端核对", async () => {
        const output = el("div", undefined, "check-result");
        check.querySelector(".check-result")?.remove();
        check.append(output);
        try {
          const results = await Promise.all(
            gs.map((g) =>
              api("/api/decision", { collaboration: r.id, action: g.action })
            )
          );
          output.textContent =
            results
              .map(
                (x, i) =>
                  `${text(gs[i].action)}：${{ autonomous: "可自主决定", confirmation_required: "需要有效确认", denied: "不允许" }[x.status] || x.status}`
              )
              .join("；") || "没有有效授权";
        } catch (e) {
          output.textContent = e.message;
        }
      })
    );
    row.append(check);
    powers.append(row);
  }
  powers.append(
    el(
      "div",
      `责任范围：${labelOf("scopes", pos.scope_id)}。上下级明确责任归属，实际操作仍须通过相应授权；配置不等于真实智能体已经接入。`,
      "qo-note"
    )
  );
  if (active(pos) && state.management_available)
    powers.append(actions(button("添加另一项协作", () => openWork(pos, null))));
  detail.append(powers);
}
function openWork(pos, r) {
  selectedPosition = pos.id;
  editing = r || null;
  $("settings").dataset.newWork = "1";
  navigate("settings");
}
function renderSettings() {
  const target = $("settings");
  target.replaceChildren();
  if (!state.management_available) {
    empty(
      target,
      "当前账号可查看获授权的工作；配置任职与范围需要另行管理授权。",
      button("查看组织关系", () => navigate("overview"))
    );
    return;
  }
  const pos = selectedOrg();
  if (!pos || !active(pos)) {
    empty(
      target,
      "请先选择有效的组织岗位。",
      button("返回组织关系", () => navigate("overview"))
    );
    return;
  }
  if (editing && (editing.role !== pos.role_id || editing.scope !== pos.scope_id))
    editing = null;
  if (editing === null && page === "settings" && !target.dataset.newWork) {
    editing = currentRelations(pos).find((r) => !r.immutable) || null;
  }
  delete target.dataset.newWork;
  const r = editing;
  if (r?.immutable) {
    empty(
      target,
      "初始化任职受保护，请选择其他岗位。",
      button("返回组织关系", () => navigate("overview"))
    );
    return;
  }
  const form = el("form", undefined, "qo-form");
  form.id = "work-form";
  form.append(
    titleRow(
      `配置${pos.label}`,
      "按岗位设置任职、工作范围和触达对象",
      button("返回组织关系", () => navigate("overview"))
    )
  );
  target.append(form);
  const one = box("1 · 谁在这个岗位工作"),
    two = el("div", undefined, "qo-two");
  const peopleBox = el("div");
  const search = inputField(
    peopleBox,
    "person-search",
    "按姓名或昵称查找",
    "",
    "search"
  );
  search.placeholder = "搜索已有人员，核对同名候选";
  const person = selectField(
    peopleBox,
    "person",
    "任职人员",
    personOptions(),
    r?.person || "",
    "请选择具体人员"
  );
  person.required = true;
  const termBox = el("div");
  const term = selectField(
    termBox,
    "term",
    "任期",
    [
      { id: "ongoing", label: "持续有效，直至撤销或离任" },
      { id: "temporary", label: "临时任职 · 指定截止时间" },
    ],
    r?.valid_until ? "temporary" : "ongoing"
  );
  let untilValue = "";
  if (r?.valid_until) {
    const d = new Date(r.valid_until);
    untilValue = new Date(d.getTime() - d.getTimezoneOffset() * 60000)
      .toISOString()
      .slice(0, 16);
  }
  const until = inputField(termBox, "until", "任期截止", untilValue, "datetime-local");
  until.parentElement.hidden = !r?.valid_until;
  term.addEventListener("change", () => {
    until.parentElement.hidden = term.value === "ongoing";
    until.required = term.value === "temporary";
  });
  two.append(peopleBox, termBox);
  one.append(
    two,
    sub("新登记但未核验的人员不会出现在任职候选中；可从基础台账查看其待核验状态。")
  );
  search.addEventListener("input", () => {
    const old = person.value;
    person.replaceChildren(new Option("请选择具体人员", ""));
    personOptions()
      .filter((p) => p.label.includes(search.value.trim()))
      .forEach((p) => {
        const o = new Option(p.label, p.id);
        o.disabled = p.disabled;
        person.add(o);
      });
    person.value = old;
  });
  form.append(one);
  const work = box("2 · 管到哪里、管理哪些智能体");
  work.append(
    el(
      "div",
      `${pos.label} · ${labelOf("scopes", pos.scope_id)}；上级岗位：${state.organization.positions.find((x) => x.id === pos.parent_id)?.label || "无"}`,
      "qo-note"
    )
  );
  const existing = currentRelations(pos).filter((x) => !x.immutable);
  if (existing.length) {
    const switcher = selectField(
      work,
      "work-choice",
      "当前工作安排",
      [
        ...existing.map((x) => ({
          id: x.id,
          label: `${personName(x.person)} · ${agentName(x.agent)} · ${labelOf("duties", x.duty)}`,
        })),
        { id: "new", label: "添加另一项协作" },
      ],
      r?.id || "new"
    );
    switcher.addEventListener("change", () => {
      editing = existing.find((x) => x.id === switcher.value) || null;
      target.dataset.newWork = "1";
      discardPreview();
      renderSettings();
    });
  }
  const workFields = el("div", undefined, "qo-two"),
    role = state.roles.find((x) => x.id === pos.role_id),
    duties = state.duties.filter((d) => active(d) && role?.duty_ids.includes(d.id));
  const duty = selectField(
    workFields,
    "duty",
    "承担的职责",
    duties,
    r?.duty || duties[0]?.id || "",
    "请选择职责"
  );
  duty.required = true;
  const agent = selectField(
    workFields,
    "agent",
    "本项工作的智能体",
    state.agents.map((id) => ({
      id,
      label: agentName(id),
      disabled: !active(ledgerFor("agent", id) || {}),
    })),
    r?.agent || "",
    "请选择智能体"
  );
  agent.required = true;
  work.append(
    workFields,
    sub(
      "每项协作分别配置智能体、职责和权限；本次保存只调整所选协作。要结束此人的全部工作，请返回组织关系选择“结束整项任职”。"
    )
  );
  if (!duties.length)
    work.append(
      button("为岗位关联职责", () => {
        ledgerKind = "role";
        ledgerSelection = pos.id;
        navigate("ledger");
      })
    );
  form.append(work);
  const audience = audienceOf(r || {}) || {
    groups: [],
    people: [],
    residents: "none",
    reply: "denied",
    proactive: "denied",
    open_reception: false,
    visibility: "general",
    topics: "",
  };
  const contacts = box("3 · 智能体可以触达哪些群和个人"),
    contactColumns = el("div", undefined, "qo-two"),
    groupBox = el("fieldset"),
    personBox = el("fieldset");
  groupBox.append(el("legend", "群"));
  const bound = state.bindings
    .filter((b) => b.scope === pos.scope_id)
    .map((b) => b.conversation);
  checkList(
    groupBox,
    "audience-groups",
    state.groups.map((g) => ({
      id: g.id,
      label: g.label + (!bound.includes(g.id) ? " · 超出范围" : ""),
      disabled: !bound.includes(g.id) || !active(ledgerFor("group", g.id) || {}),
    })),
    audience.groups
  );
  if (!bound.length) groupBox.append(sub("此范围尚未绑定群。请在群台账维护范围关联。"));
  personBox.append(el("legend", "具体个人"));
  const peopleSearch = inputField(
    personBox,
    "audience-search",
    "查找联系对象",
    "",
    "search"
  );
  const contactList = checkList(
    personBox,
    "audience-people",
    personOptions(),
    audience.people
  );
  peopleSearch.addEventListener("input", () =>
    contactList
      .querySelectorAll("label")
      .forEach((l) => (l.hidden = !l.textContent.includes(peopleSearch.value.trim())))
  );
  contactColumns.append(groupBox, personBox);
  contacts.append(contactColumns);
  selectField(
    contacts,
    "residents",
    "动态人员集合",
    [
      { id: "none", label: "不选择动态对象" },
      { id: "current", label: "本范围在住人员" },
      { id: "past", label: "本范围过往人员" },
      { id: "all", label: "本范围在住与过往全部人员" },
    ],
    audience.residents
  );
  contacts.append(
    sub(
      "动态集合按可靠住宿及身份关联解析，当前 PMS 消费尚未接入。“全部”不包含范围外人员或整个通讯录。"
    )
  );
  const modeFields = el("div", undefined, "qo-two"),
    modes = Object.entries(modeNames).map(([id, label]) => ({ id, label }));
  const reply = selectField(
      modeFields,
      "reply",
      "回复对方主动联系",
      modes,
      audience.reply
    ),
    proactive = selectField(
      modeFields,
      "proactive",
      "主动联系（仍需发布授权）",
      modes,
      audience.proactive
    );
  contacts.append(modeFields);
  const contactReviewer = selectField(
    contacts,
    "contact-reviewer",
    "触达确认人",
    personOptions().filter((p) => p.id !== person.value && !p.disabled),
    audience.reviewer || "",
    "请选择有对应审核自主权的人员"
  );
  const contactModeChanged = () => {
    const needed = reply.value === "confirmation" || proactive.value === "confirmation";
    contactReviewer.parentElement.hidden = !needed;
    contactReviewer.required = needed;
    if (!needed) contactReviewer.value = "";
  };
  reply.addEventListener("change", contactModeChanged);
  proactive.addEventListener("change", contactModeChanged);
  contactModeChanged();
  const more = details("公开咨询、内容边界与信息可见性");
  selectField(
    more,
    "open-reception",
    "公开咨询接待",
    [
      { id: "no", label: "此项工作不承担开放接待" },
      { id: "yes", label: "接待所有主动联系者，仅限公开咨询" },
    ],
    audience.open_reception ? "yes" : "no"
  );
  inputField(
    more,
    "topics",
    "联系目的与内容边界",
    audience.topics || r?.responsibility || pos.description,
    "textarea",
    true
  );
  selectField(
    more,
    "visibility",
    "信息可见性",
    [
      { id: "general", label: "范围内一般服务信息" },
      { id: "service_private", label: "仅事项必要的服务信息（消费端仍须核验）" },
    ],
    audience.visibility
  );
  contacts.append(more);
  form.append(contacts);
  const powers = box("4 · 职责内的决定权"),
    permissions = el("div");
  permissions.id = "permissions";
  powers.append(permissions);
  const delegation = details("可交给其他人的管理范围");
  delegation.id = "delegation";
  const envelope = state.grants.find(
    (g) => g.collaboration === r?.id && g.action === "manage" && g.revocable
  )?.management_envelope;
  delegation.append(
    sub(
      "管理权与本人执行权分开。仅在这里明确选中的智能体、领域和权限可继续分配；包含当前范围内的下级范围。"
    )
  );
  checkList(
    delegation,
    "managed-agents",
    state.agents.map((id) => ({ id, label: agentName(id) })),
    envelope?.agents
  );
  checkList(
    delegation,
    "managed-domains",
    state.domains.map((id) => ({ id, label: text(id) })),
    envelope?.domains
  );
  checkList(
    delegation,
    "managed-actions",
    state.actions.map((id) => ({ id, label: text(id) })),
    envelope?.actions
  );
  selectField(
    delegation,
    "depth",
    "下级继续分配层数",
    [0, 1, 2, 3].map((x) => ({ id: String(x), label: String(x) })),
    String(envelope?.depth || 0)
  );
  powers.append(
    delegation,
    sub(
      "岗位与职责定义不会自动增加已有权限。选择确认人只是设置确认条件，不代表其已批准内容。"
    )
  );
  form.append(powers);
  const permissionRows = () => {
    const d = duties.find((x) => x.id === duty.value);
    permissions.replaceChildren();
    for (const key of d?.available_actions || []) {
      const current = state.grants.find(
          (g) => g.collaboration === r?.id && g.action === key && g.revocable
        ),
        row = el("div", undefined, "qo-ability"),
        desc = el("div");
      desc.append(el("label", text(key)));
      desc.firstChild.htmlFor = "permission-" + key;
      const controls = el("div");
      const mode = selectField(
        controls,
        "permission-" + key,
        "决定方式",
        modes,
        current?.mode || "denied"
      );
      mode.setAttribute("aria-label", text(key));
      const reviewer = selectField(
        controls,
        "reviewer-" + key,
        "确认人",
        personOptions().filter((p) => p.id !== person.value && !p.disabled),
        current?.reviewer || "",
        "选择确认人"
      );
      reviewer.setAttribute("aria-label", text(key) + "的确认人");
      const update = () => {
        reviewer.parentElement.hidden = mode.value !== "confirmation";
        reviewer.required = mode.value === "confirmation";
        if (mode.value !== "confirmation") reviewer.value = "";
        delegation.hidden = $("permission-manage")?.value !== "autonomous";
      };
      mode.addEventListener("change", update);
      row.append(desc, controls);
      permissions.append(row);
      update();
    }
    delegation.hidden = $("permission-manage")?.value !== "autonomous";
  };
  duty.addEventListener("change", permissionRows);
  permissionRows();
  person.addEventListener("change", () => {
    form.querySelectorAll('select[id^="reviewer-"],#contact-reviewer').forEach((s) => {
      for (const o of s.options) o.disabled = o.value === person.value;
      if (s.value === person.value) s.value = "";
    });
  });
  const advanced = details("工作说明与临时代理");
  inputField(
    advanced,
    "responsibility",
    "工作说明",
    r?.responsibility || pos.description,
    "textarea",
    true
  );
  selectField(
    advanced,
    "proxy",
    "临时代理哪项任职",
    [
      ...new Map(
        state.relations
          .filter(
            (x) =>
              x.status === "active" &&
              x.role === pos.role_id &&
              x.scope === pos.scope_id &&
              x.person !== person.value
          )
          .map((x) => [
            x.appointment,
            { id: x.appointment, label: personName(x.person) },
          ])
      ).values(),
    ],
    r?.proxy_for || "",
    "不是临时代理"
  );
  form.append(advanced);
  const submit = el("button", "预览配置影响", "qo-primary");
  submit.type = "submit";
  form.append(
    actions(
      submit,
      button("取消本次修改", () => navigate("overview"))
    )
  );
  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const d = duties.find((x) => x.id === duty.value);
    if (!d) return notice("请先选择岗位承担的职责。", true);
    const assignment = {
      collaboration: r?.id || null,
      person: person.value,
      role: pos.role_id,
      duty: d.id,
      scope: pos.scope_id,
      agent: agent.value,
      domain: d.domain,
      responsibility: $("responsibility").value,
      valid_until:
        term.value === "temporary" && until.value
          ? new Date(until.value).toISOString()
          : null,
      proxy_for: $("proxy").value || null,
      actions: [],
      permissions: d.available_actions.map((action) => ({
        action,
        mode: $("permission-" + action).value,
        reviewer: $("reviewer-" + action).value || null,
      })),
      delegation:
        $("permission-manage")?.value === "autonomous"
          ? {
              agents: selected("managed-agents"),
              domains: selected("managed-domains"),
              actions: selected("managed-actions"),
              depth: Number($("depth").value),
            }
          : null,
    };
    const a = {
      groups: selected("audience-groups"),
      people: selected("audience-people"),
      residents: $("residents").value,
      reply: reply.value,
      proactive: proactive.value,
      reviewer: contactReviewer.value || null,
      topics: $("topics").value,
      visibility: $("visibility").value,
      open_reception: $("open-reception").value === "yes",
    };
    preview(
      { kind: "configure_work", assignment, audience: a },
      [
        ["任职人员", personName(assignment.person)],
        ["岗位与范围", `${pos.label} / ${labelOf("scopes", pos.scope_id)}`],
        ["智能体与职责", `${agentName(assignment.agent)} / ${d.label}`],
        [
          "任期",
          assignment.valid_until
            ? new Date(assignment.valid_until).toLocaleString()
            : "持续有效，直至撤销或离任",
        ],
        ["群触达", a.groups.map((x) => labelOf("groups", x)).join("、") || "无"],
        ["个人触达", a.people.map(personName).join("、") || "无"],
        ["触达方式", audienceSummary(a)],
        [
          "决定权",
          assignment.permissions
            .map(
              (p) =>
                `${text(p.action)}：${modeNames[p.mode]}${p.reviewer ? `（${personName(p.reviewer)}）` : ""}`
            )
            .join("；"),
        ],
        [
          "生效方式",
          r && r.person !== assignment.person
            ? "本项工作换任，旧连接及权限立即失效；其他工作不变。触达与权限在同一事务保存。"
            : "本项工作、权限和触达在同一事务保存；不触发真实消息。",
        ],
      ],
      form
    );
  });
}
