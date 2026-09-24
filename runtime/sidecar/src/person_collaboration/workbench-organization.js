function selectedOrg() {
  return (
    state.organization.positions.find((x) => x.id === selectedPosition) ||
    state.organization.positions.find((x) => x.label === "社区负责人") ||
    state.organization.positions[0]
  );
}
function renderOrganization() {
  if (!state.management_available) {
    renderPersonalWork();
    return;
  }
  const target = $("overview");
  target.replaceChildren();
  const layout = el("div", undefined, "qo-layout"),
    tree = el("aside", undefined, "qo-tree"),
    detail = el("div");
  tree.setAttribute("aria-label", "组织与岗位结构");
  tree.append(el("h3", "秦托邦"), sub("选择岗位，查看负责人和工作边界"));
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
            ? [
                ...new Set(
                  rs.map(
                    (r) =>
                      personName(r.person) +
                      (r.status === "scheduled" ? "（待开始）" : "")
                  )
                ),
              ].join("、") || "空缺 · 待安排"
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
  const manage = box("谁负责什么"),
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
  explainHeading(
    manage,
    "责任与决定权",
    "上下级岗位说明责任归属，不能代替业务授权。下方分别列出已生效决定权、未来安排和共同约束。"
  );
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
    if (a) row.append(audiencePreview(r));
    contact.append(row);
  }
  detail.append(contact);
  const powers = box("能决定什么、何时生效");
  explainHeading(
    powers,
    "决定方式",
    "自主决定只在已授权职责和范围内有效。需要指定人确认时，对方还须持有当前有效的对应权限；未授予的事项不能执行。"
  );
  if (!rs.length) empty(powers, "空缺岗位没有可执行的业务权限。");
  for (const r of rs) {
    const row = el("div", undefined, "qo-scope-row");
    row.append(
      el("h4", `${personName(r.person)} · ${agentName(r.agent)}`),
      el("p", r.responsibility || "工作说明尚未填写"),
      sub(termSummary(r))
    );
    const gs = grants.filter((g) => g.collaboration === r.id);
    if (r.status === "scheduled") {
      row.append(el("p", "这项任职尚未开始，当前不能使用它的权限。", "qo-note"));
      const scheduled = state.grants.filter(
        (g) => g.collaboration === r.id && g.revocable
      );
      if (scheduled.length)
        row.append(
          details(
            "已保存的开始后安排",
            ...scheduled.map((g) =>
              sub(
                `${text(g.action)}：${modeNames[g.mode]}${g.reviewer ? `，由${personName(g.reviewer)}确认` : ""}`
              )
            )
          )
        );
    } else if (!gs.length)
      row.append(sub("当前没有有效决定权，请先核对身份、任期与业务授权。"));
    for (const g of gs)
      row.append(
        el(
          "div",
          `${modeNames[g.mode]}：${text(g.action)}${g.reviewer ? ` · ${personName(g.reviewer)}` : ""}`
        )
      );
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
                  `结束此任职的 ${state.relations.filter((x) => x.appointment === r.appointment && ["active", "scheduled"].includes(x.status)).length} 项协作，立即收回对应权限并取消尚未开始的安排。历史保留。`,
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
  detail.append(powers, constraintsPanel(pos.scope_id));
  if (
    state.relations.some(
      (r) =>
        r.person === state.actor_person &&
        r.status === "active" &&
        r.agent === "erhua" &&
        r.domain === "community_service"
    )
  ) {
    const business = details("办理本范围约定与审批");
    business.addEventListener("toggle", () => {
      if (business.open && !business.dataset.loaded) {
        business.dataset.loaded = "true";
        business.append(personalRulePanel(pos.scope_id));
      }
    });
    detail.append(business);
  }
}
function renderPersonalWork() {
  const target = $("overview");
  target.replaceChildren();
  const own = state.relations.filter(
    (r) => r.person === state.actor_person && ["active", "scheduled"].includes(r.status)
  );
  const name = state.actor_person ? personName(state.actor_person) : "当前账号";
  target.append(titleRow("我的工作", `${name}，这里只显示与你有关的工作。`));
  if (state.delegated_reviews?.length) target.append(stewardProgress());
  if (!own.length) {
    if (state.delegated_reviews?.length) return;
    empty(
      target,
      "你目前没有已安排的工作。请联系负责人安排岗位和范围；账号开通后不会自动获得工作权限。"
    );
    return;
  }
  for (const relation of own) {
    const work = box(
      `${labelOf("scopes", relation.scope)} · ${labelOf("roles", relation.role)}`
    );
    work.classList.add("qo-personal-work");
    work.append(
      el("p", relation.responsibility || labelOf("duties", relation.duty)),
      sub(`与你协作：${agentName(relation.agent)}`)
    );
    if (relation.status === "scheduled") {
      work.append(
        el("p", `尚未开始 · ${displayTime(relation.valid_from)}起任职`, "qo-note"),
        sub("到开始时间并且授权仍有效后，才能办理这项工作。")
      );
      target.append(work);
      continue;
    }
    if (relation.agent === "erhua" && relation.domain === "community_service")
      work.append(stewardWorkspace(relation.scope));
    const grants = state.grants.filter(
      (g) => g.collaboration === relation.id && g.effective && g.mode !== "denied"
    );
    const summary = el("div", undefined, "qo-personal-powers");
    summary.append(
      el("h4", "我能决定什么"),
      helpTip(
        "已有决定权",
        "这些权限由负责人授予。你可以在权限内安排工作习惯和具体事项；查看此页不会新增权限，也不能把工作范围扩大到其他楼栋。"
      )
    );
    const list = el("ul");
    for (const grant of grants)
      list.append(
        el(
          "li",
          `${text(grant.action)}${grant.mode === "confirmation" ? `（需${personName(grant.reviewer)}确认）` : ""}`
        )
      );
    if (grants.length) {
      summary.append(list);
      work.append(summary);
    } else work.append(sub("当前没有可用的决定权，请联系负责人核对安排。"));
    const term = box("任期与工作依据");
    term.append(sub(termSummary(relation)), constraintsPanel(relation.scope));
    const info = el("div");
    info.append(
      button("查看任期与工作依据", () => {
        term.hidden = !term.hidden;
      })
    );
    term.hidden = true;
    info.append(term);
    work.append(info);
    target.append(work);
  }
  if (
    own.some(
      (r) =>
        r.agent === "erhua" && r.domain === "community_service" && r.status === "active"
    )
  )
    return;
  const next = box("怎样开始工作");
  next.append(
    el(
      "p",
      "日常事项直接告诉与你协作的智能体。它应说明处理结果，或告诉你还缺什么条件。"
    ),
    sub("当前为本地合成体验，真实对话渠道尚未启用。")
  );
  if (own.some((r) => r.agent === "erhua" && r.status === "active"))
    next.append(
      sub(
        "例如，对二花说“本栋有什么规则”或“把本栋厨房关闭时间改成晚上十点”。是否可以修改，仍按上面的决定权核验。"
      )
    );
  if (
    state.local_dialogue_available &&
    own.some((r) => r.agent === "erhua" && r.status === "active")
  ) {
    const link = el("a", "体验与二花对话（本地）", "qo-primary qo-work-link");
    link.href = "/foundation";
    next.append(
      actions(link),
      sub("此入口用于本批合成体验，使用固定例句理解，只产生本地回执。")
    );
  }
  target.append(next);
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
  let startMode = null,
    from = null;
  if (r) {
    termBox.append(
      pair("任职开始", displayTime(r.valid_from)),
      sub("已有任职的开始时间保留历史。如需未来安排，请另加一项任职。")
    );
  } else {
    startMode = selectField(
      termBox,
      "start-mode",
      "从什么时候开始",
      [
        { id: "now", label: "保存后立即开始" },
        { id: "scheduled", label: "指定未来时间" },
      ],
      "now"
    );
    fieldExplanation(
      startMode,
      "未来任期",
      "未来任职可以提前保存，但开始时间之前不会产生可执行权限。保存不等于提前赋权。"
    );
    from = inputField(termBox, "from", "开始时间（本地时间）", "", "datetime-local");
    from.parentElement.hidden = true;
    startMode.addEventListener("change", () => {
      from.parentElement.hidden = startMode.value !== "scheduled";
      from.required = startMode.value === "scheduled";
      if (from.required) from.min = localDateTime(new Date().toISOString());
    });
  }
  const term = selectField(
    termBox,
    "term",
    "任期到什么时候",
    [
      { id: "ongoing", label: "持续至离任或撤销" },
      { id: "temporary", label: "指定截止时间" },
    ],
    r?.valid_until ? "temporary" : "ongoing"
  );
  const until = inputField(
    termBox,
    "until",
    "截止时间（本地时间）",
    localDateTime(r?.valid_until),
    "datetime-local"
  );
  until.parentElement.hidden = !r?.valid_until;
  until.required = !!r?.valid_until;
  term.addEventListener("change", () => {
    until.parentElement.hidden = term.value === "ongoing";
    until.required = term.value === "temporary";
  });
  fieldExplanation(
    term,
    "任期截止",
    "到期后，对应决定权失效。临时代理必须有截止时间；截止应晚于开始，且不能已经到期。历史审批不会改写为新人的决定。"
  );
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
  form.append(work, constraintsPanel(pos.scope_id));
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
  explainHeading(
    contacts,
    "联系范围",
    "能联系谁与能向对方说什么分别核对。选择群或个人不代表已发送，也不会开放额外私人信息；主动联系仍需要发布授权。"
  );
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
  const residents = selectField(
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
      "动态对象按本范围的可靠住宿与已确认身份解析。“全部”指在住和过往入住人员，不包含整个通讯录；资料不确定时保留待核对状态。"
    )
  );
  if (r) contacts.append(audiencePreview(r, true));
  else contacts.append(sub("先保存这项工作，再核对它实际覆盖的人员。"));
  fieldExplanation(
    residents,
    "动态人员范围",
    "名单随可靠住宿事实变化，执行时重新核对。当前在住、过往入住和资料不明会分别显示；历史入住不等于目前仍在住。"
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
  const openReception = selectField(
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
  const visibility = selectField(
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
  fieldExplanation(
    openReception,
    "公开咨询接待",
    "允许回应主动咨询的外部人员，只限公开信息。它不会把对方加入主动通知对象，也不会扩大可查看的内容。"
  );
  fieldExplanation(
    visibility,
    "信息可见性",
    "这是这项工作的内容边界，实际消费端仍逐项核验用途和权限。可联系某人不等于可以把全部资料告诉他。"
  );
  form.append(contacts);
  const powers = box("4 · 职责内的决定权"),
    permissions = el("div");
  permissions.id = "permissions";
  powers.append(permissions);
  explainHeading(
    powers,
    "职责内的决定权",
    "按具体操作分别授予。选择指定确认人只是建立确认条件，不代表对方已批准某份内容；确认人也必须拥有对应的当前自主权。"
  );
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
  const depth = selectField(
    delegation,
    "depth",
    "下级继续分配层数",
    [0, 1, 2, 3].map((x) => ({ id: String(x), label: String(x) })),
    String(envelope?.depth || 0)
  );
  fieldExplanation(
    depth,
    "继续分配权限",
    "0 表示接到管理授权的人不能再转授；更高层数允许继续向下分配，但每一层都不能超出上层明确给出的智能体、领域和权限。"
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
  const proxy = selectField(
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
  fieldExplanation(
    proxy,
    "临时代理",
    "代理须对应另一人的同岗位同范围任职，并明确截止时间。它不会继承未授予的权限，也不会改写原任职记录。"
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
    const startsAt = r
      ? null
      : startMode.value === "scheduled" && from.value
        ? new Date(from.value)
        : null;
    if (
      !r &&
      startMode.value === "scheduled" &&
      (!startsAt || Number.isNaN(startsAt.getTime()) || startsAt <= new Date())
    ) {
      notice("请选择晚于现在的任职开始时间。", true);
      from.focus();
      return;
    }
    const endsAt =
      term.value === "temporary" && until.value ? new Date(until.value) : null;
    const effectiveStart =
      startsAt || (r?.valid_from ? new Date(r.valid_from) : new Date());
    if (
      term.value === "temporary" &&
      (!endsAt ||
        Number.isNaN(endsAt.getTime()) ||
        endsAt <= effectiveStart ||
        endsAt <= new Date())
    ) {
      notice("截止时间须晚于任职开始，且不能已经到期。", true);
      until.focus();
      return;
    }
    const assignment = {
      collaboration: r?.id || null,
      person: person.value,
      role: pos.role_id,
      duty: d.id,
      scope: pos.scope_id,
      agent: agent.value,
      domain: d.domain,
      responsibility: $("responsibility").value,
      valid_from: startsAt ? startsAt.toISOString() : null,
      valid_until: endsAt ? endsAt.toISOString() : null,
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
          `${r ? `沿用 ${displayTime(r.valid_from)} 的开始时间` : assignment.valid_from ? `${displayTime(assignment.valid_from)} 开始` : "保存后立即开始"}；${assignment.valid_until ? `${displayTime(assignment.valid_until)} 截止` : "持续至离任或撤销"}`,
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
          effectiveStart > new Date()
            ? "未来安排先保存，开始前没有可执行权限；届时仍需核验身份、职责与授权。"
            : r && r.person !== assignment.person
              ? "本项工作换任，旧连接及权限立即失效；其他工作不变。触达与权限在同一事务保存。"
              : "本项工作、权限和触达在同一事务保存；不触发真实消息。",
        ],
      ],
      form
    );
  });
}

// Business decisions always use scoped rule authority, independent of organization management.
function personalRulePanel(scope, kind = "rule") {
  const noun = kind === "rule" ? "约定" : "知识";
  const permission = kind === "rule" ? "change_rules" : "confirm_knowledge";
  const panel = box(`本栋${noun}`),
    content = el("div"),
    status = sub(`正在读取本栋${noun}……`);
  status.setAttribute("role", "status");
  panel.append(
    sub(
      kind === "rule"
        ? `安排本栋的表达习惯、联系与提醒方式，或具体工作${noun}。`
        : "把设施说明、生活方法和本栋文化交给二花；支持导入 Markdown、修改、设定期限和停止使用。"
    ),
    content
  );
  explainHeading(
    panel,
    `本栋${noun}`,
    `停止使用会取消整项${noun}及其未来安排，历史仍可查看。只取消一项未来安排，不影响当前内容。到截止时间自动结束，不恢复旧版。不会自动发送群通知。`
  );
  if (!state.local_dialogue_available) {
    content.append(sub("当前本地业务入口未启用。"));
    return panel;
  }
  let data,
    editor = null,
    working = false;
  const titleOf = (item) =>
    item?.content?.title ||
    item?.revisions?.[0]?.content?.title ||
    (item?.key === "kitchen" ? "厨房使用" : `本栋工作${noun}`);
  const dates = (r) =>
    `${r.effective_at ? displayTime(r.effective_at) : "批准或保存后立即"}起 · ${r.effective_until ? `${displayTime(r.effective_until)}截止` : "不设截止"}`;
  const editable = () =>
    data.context.permissions.some(
      (p) =>
        p.action === permission &&
        ["autonomous", "confirmation_required"].includes(p.decision.status)
    );
  const localTime = (value) => {
    if (!value) return "";
    const date = new Date(value);
    return new Date(date.getTime() - date.getTimezoneOffset() * 60000)
      .toISOString()
      .slice(0, 16);
  };
  const load = async () => {
    data = await api("/api/foundation/rules", { scope });
    data.items = data.items.filter((i) =>
      kind === "rule" ? i.kind === "rule" : i.kind !== "rule"
    );
    data.tasks = data.tasks.filter((t) =>
      kind === "rule"
        ? (t.input.lifecycle?.kind || "rule") === "rule"
        : (t.input.lifecycle?.kind || "rule") !== "rule"
    );
  };
  const refresh = async () => {
    if (working) return;
    working = true;
    panel.inert = true;
    try {
      await load();
      draw();
    } catch (e) {
      status.textContent = e.message;
    } finally {
      working = false;
      panel.inert = false;
    }
  };
  const resultText = (r) =>
    r.replayed
      ? "已核对原请求结果，请查看下面的当前状态。"
      : {
          saved: `${noun}已保存。`,
          stopped: `整项${noun}已停止，未来安排也已取消。`,
          schedule_cancelled: "已取消这项未来安排。",
          awaiting_review: `已提交指定人确认，可在${noun}请求中查看或撤销。`,
          completed: "已批准并执行。",
          failed: `未执行：${errors[r.reason] || "原请求条件已变化，请重新核对。"}`,
          cancelled: `请求已结束，未修改${noun}。`,
        }[r.status] || "请求已记录，请核对下方状态。";
  async function decide(task, action) {
    if (working) return;
    working = true;
    panel.inert = true;
    try {
      const result = await api("/api/foundation/rule/decision", {
        scope,
        work_item_id: task.id,
        action,
      });
      await load();
      draw();
      status.textContent = resultText(result);
    } catch (e) {
      status.textContent = e.message;
    } finally {
      working = false;
      panel.inert = false;
    }
  }
  function draw() {
    editor = null;
    content.replaceChildren(status);
    status.textContent = "";
    const decision = data.context.permissions.find(
      (p) => p.action === permission
    )?.decision;
    content.append(
      sub(
        decision?.status === "autonomous"
          ? `你可以直接安排本栋${noun}。`
          : decision?.status === "confirmation_required"
            ? `你的修改需${decision.reviewer_name || personName(decision.reviewer)}确认后生效。`
            : `你可以查看本栋${noun}；修改权限请联系负责人核对。`
      )
    );
    const ended = box("已结束或停止的记录");
    const live = el("div");
    content.append(
      workTabs("记录状态", [
        ["当前与待生效", live],
        ["已结束", ended],
      ])
    );
    let endedCount = 0;
    for (const item of data.items) {
      const row = el("article", undefined, "qo-scope-row");
      const current = item.current;
      row.append(el("h4", titleOf(current || item)));
      if (current)
        row.append(el("p", current.content.text), sub(`当前生效 · ${dates(current)}`));
      else
        row.append(
          sub(
            item.stopped_at
              ? `已停止 · ${displayTime(item.stopped_at)}`
              : item.scheduled.length
                ? "等待开始，目前没有生效内容。"
                : "当前未生效（已结束或原授权已变化），不会使用旧版。"
          )
        );
      if (editable()) {
        if (current)
          row.append(actions(button(`修改${noun}`, () => edit(item, current))));
        else if (!item.scheduled.length)
          row.append(
            actions(
              button(item.stopped_at ? "重新启用" : "重新安排", () =>
                edit(
                  item,
                  item.revisions
                    .filter(
                      (r) =>
                        new Date(r.effective_at) <=
                        new Date(item.stopped_at || Date.now())
                    )
                    .sort(
                      (a, b) =>
                        new Date(b.effective_at) - new Date(a.effective_at) ||
                        b.version - a.version
                    )[0] || item.revisions[0],
                  true
                )
              )
            )
          );
        if (!item.stopped_at)
          row.append(
            actions(
              button(`停止整项${noun}`, () =>
                confirmChange(
                  item,
                  { action: "stop" },
                  `停止“${titleOf(item)}”后，当前内容及全部未来安排都不再使用。历史记录保留。`
                )
              )
            )
          );
      }
      for (const later of item.scheduled) {
        const scheduled = el("div", undefined, "qo-note");
        scheduled.append(
          el("strong", `待生效 · ${titleOf(later)}`),
          el("p", later.content.text),
          sub(dates(later))
        );
        if (editable())
          scheduled.append(
            actions(
              button("修改未来安排", () => edit(item, later, false, true)),
              button("取消这次安排", () =>
                confirmChange(
                  item,
                  { action: "cancel_scheduled", revision_id: later.id },
                  `仅取消 ${dates(later)} 的未来安排，当前${noun}保持不变。`
                )
              )
            )
          );
        row.append(scheduled);
      }
      const currentView = el("div");
      while (row.firstChild) currentView.append(row.firstChild);
      row.append(
        workTabs(titleOf(item), [
          ["内容与安排", currentView],
          ["历史记录", knowledgeHistory(item, dates)],
        ])
      );
      if (!current && !item.scheduled.length) {
        ended.append(row);
        endedCount++;
      } else live.append(row);
    }
    if (!data.items.length)
      content.append(sub(`尚无本栋${noun}，可以从新增一项开始。`));
    if (!endedCount) ended.append(sub("暂无已结束记录。"));
    content.append(
      actions(
        ...(editable()
          ? [button(`新增${noun}`, () => edit(null, null), "qo-primary")]
          : []),
        button(`刷新${noun}与请求`, refresh)
      )
    );
    if (data.tasks.length) {
      const tasks = box(`${noun}请求`);
      tasks.append(
        sub(
          "这里只显示你提出的请求，以及当前指定由你确认的请求。批准时会再次核对权限、日期和版本。"
        )
      );
      for (const task of data.tasks) {
        const command = task.input.lifecycle;
        const change = command?.change || {
          action: "save",
          content: task.input.content,
          effective_at: task.input.effective_at,
          effective_until: task.input.effective_until,
        };
        const item = data.items.find((i) => i.key === (command?.key || task.input.key));
        const row = el("article", undefined, "qo-scope-row");
        const names = {
          awaiting_review: "等待确认",
          queued: "已批准，待执行",
          completed: "已完成",
          failed: "未执行",
          cancelled: "已结束",
        };
        row.append(
          el("h4", `${task.requester} · ${names[task.status] || task.status}`),
          sub(
            `指定确认人：${task.reviewer ? task.reviewer_name || personName(task.reviewer) : "当前无有效确认人"}`
          )
        );
        if (item?.current)
          row.append(
            el("p", `当前：${titleOf(item.current)} · ${item.current.content.text}`),
            sub(dates(item.current))
          );
        if (change.action === "save")
          row.append(
            el(
              "p",
              `申请${change.reactivate ? "重新启用" : change.replace_revision ? "替换未来安排" : "保存"}：${change.content.title || titleOf(item)} · ${change.content.text}`
            ),
            sub(dates(change))
          );
        else
          row.append(
            el(
              "p",
              change.action === "stop"
                ? `申请停止整项“${titleOf(item)}”及全部未来安排。`
                : `申请取消“${titleOf(item)}”的一项未来安排。`
            )
          );
        if (change.action === "cancel_scheduled") {
          const revision = item?.revisions.find((r) => r.id === change.revision_id);
          if (revision)
            row.append(el("p", revision.content.text), sub(dates(revision)));
        }
        if (task.input.before?.revisions?.length) {
          row.append(knowledgeHistory(task.input.before, dates));
        }
        if (task.reason)
          row.append(sub(errors[task.reason] || `原请求条件已变化，未修改${noun}。`));
        row.append(
          actions(
            ...(task.can_review || task.can_execute
              ? [
                  button(
                    task.can_execute ? "继续执行已批准请求" : "批准并执行以上内容",
                    () => decide(task, "approve"),
                    "qo-primary"
                  ),
                ]
              : []),
            ...(task.can_review
              ? [button("拒绝请求", () => decide(task, "reject"))]
              : []),
            ...(task.can_cancel
              ? [button("撤销我的请求", () => decide(task, "cancel"))]
              : [])
          )
        );
        tasks.append(row);
      }
      content.append(tasks);
    }
  }
  function openEditor(heading) {
    editor?.remove();
    editor = el("form", undefined, "qo-work-editor");
    editor.append(el("h4", heading));
    content.append(editor);
    return editor;
  }
  function attachSubmit(form, item, buildChange, label) {
    const feedback = sub("");
    feedback.setAttribute("role", "status");
    const save = el("button", label, "qo-primary");
    save.type = "submit";
    const retry = button("重试原请求（内容保持不变）", async () => send(frozen));
    retry.hidden = true;
    const reread = button("重新读取并核对（保留草稿）", async () => {
      panel.inert = true;
      try {
        await load();
        const latest = data.items.find((i) => i.key === key);
        feedback.textContent = latest?.current
          ? `当前${noun}：${latest.current.content.text} · ${dates(latest.current)}`
          : "当前无此项生效内容，请核对是否停止或已排期。";
        if (uncertain) {
          retry.hidden = false;
          feedback.textContent +=
            " 原提交结果未明，可重试同一请求获取回执，不会重复创建。";
        } else {
          version = latest?.version || 0;
          frozen = null;
          save.disabled = !editable();
          feedback.textContent += " 草稿保留，请核对后再保存。";
        }
      } catch (e) {
        feedback.textContent = e.message;
      } finally {
        panel.inert = false;
      }
    });
    reread.hidden = true;
    let version = item?.version || 0,
      frozen = null,
      uncertain = false;
    const key = item?.key || `local_${crypto.randomUUID()}`;
    async function send(command) {
      if (working || panel.inert) return;
      working = true;
      panel.inert = true;
      let acknowledged = false;
      try {
        const result = await api("/api/foundation/rule/change", command);
        acknowledged = true;
        await load();
        draw();
        status.textContent = resultText(result) + "未发送群通知。";
      } catch (e) {
        uncertain = acknowledged || !e.code;
        save.disabled = true;
        reread.hidden = false;
        retry.hidden = true;
        feedback.textContent = acknowledged
          ? "已收到保存回执，但重新读取失败。请先核对状态。"
          : e.message;
      } finally {
        working = false;
        panel.inert = false;
      }
    }
    form.append(
      actions(
        save,
        button("取消编辑", () => {
          form.remove();
          editor = null;
        })
      ),
      feedback,
      reread,
      retry
    );
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      if (working || save.disabled || panel.inert) return;
      try {
        frozen = {
          scope,
          key,
          operation_id: crypto.randomUUID(),
          expected_version: version,
          kind: item?.kind || form.dataset.knowledgeKind || kind,
          change: buildChange(),
        };
        await send(frozen);
      } catch (e) {
        feedback.textContent = e.message;
      }
    });
  }
  function confirmChange(item, change, description) {
    const form = openEditor(
      change.action === "stop" ? `确认停止整项${noun}` : "确认取消未来安排"
    );
    form.append(el("p", description));
    attachSubmit(form, item, () => change, "确认提交");
    form.querySelector('button[type="submit"]').focus();
  }
  function edit(item, revision, restart = false, scheduled = false) {
    const form = openEditor(
      !item
        ? `新增本栋${noun}`
        : restart
          ? `重新安排${noun}`
          : scheduled
            ? "修改未来安排"
            : `修改本栋${noun}`
    );
    const prefix = crypto.randomUUID();
    const title = inputField(
      form,
      `${prefix}-title`,
      `${noun}名称`,
      revision ? titleOf(revision) : "",
      "text",
      true
    );
    const body = inputField(
      form,
      `${prefix}-body`,
      `具体${noun}`,
      revision?.content.text || "",
      "textarea",
      true
    );
    body.maxLength = 12000;
    if (kind !== "rule") {
      const category = selectField(
        form,
        `${prefix}-kind`,
        "资料类型",
        [
          { id: "fact", label: "设施与生活说明" },
          { id: "culture", label: "本栋文化" },
          { id: "experience", label: "经验建议" },
        ],
        item?.kind || "fact"
      );
      category.disabled = !!item;
      form.dataset.knowledgeKind = category.value;
      category.addEventListener("change", () => {
        form.dataset.knowledgeKind = category.value;
      });
      const file = inputField(form, `${prefix}-file`, "导入 Markdown 文档", "", "file");
      file.accept = ".md,.markdown,text/markdown,text/plain";
      const hint = sub(
        "支持 UTF-8 Markdown，最多 12 KB。导入后可以编辑，保存后才供二花使用；文档中的指令不会授予权限。"
      );
      file.addEventListener("change", async () => {
        const selected = file.files?.[0];
        if (!selected) return;
        if (!/\.(md|markdown)$/i.test(selected.name) || selected.size > 12000) {
          hint.textContent = "请选择不超过 12 KB 的 Markdown 文件。";
          return;
        }
        try {
          body.value = new TextDecoder("utf-8", { fatal: true }).decode(
            await selected.arrayBuffer()
          );
          if (!title.value)
            title.value = selected.name.replace(/\.(md|markdown)$/i, "").slice(0, 80);
          hint.textContent = "已读入草稿，请核对内容后保存。";
        } catch {
          hint.textContent = "无法读取，请使用 UTF-8 编码的 Markdown 文件。";
        }
      });
      form.append(hint);
    }
    title.placeholder = "例如：厨房使用";
    body.placeholder = "写清楚本栋希望如何安排。";
    const start = inputField(
      form,
      `${prefix}-start`,
      "开始时间（留空即保存或批准后生效）",
      scheduled ? localTime(revision.effective_at) : "",
      "datetime-local"
    );
    const end = inputField(
      form,
      `${prefix}-end`,
      "结束时间（留空表示不设截止）",
      restart ? "" : localTime(revision?.effective_until),
      "datetime-local"
    );
    fieldExplanation(
      end,
      `${noun}何时结束`,
      "截止后不再使用，也不会恢复更早的版本。需要恢复时，请明确重新安排。日期按你设备的本地时间显示。审批完成前不会生效。"
    );
    fieldExplanation(
      start,
      "当前与未来安排",
      scheduled
        ? `保存会替换所选未来版本。若它已经开始生效，请刷新后按当前${noun}修改。`
        : `修改当前${noun}不会取消已有未来安排，请同时核对它们；若要全部停止，请使用停止整项${noun}。`
    );
    if (item?.stopped_at)
      form.append(sub(`你正在明确重新启用已停止的${noun}；旧的未来安排不会恢复。`));
    attachSubmit(
      form,
      item,
      () => {
        if (!title.value.trim() || !body.value.trim())
          throw new Error("请填写名称和具体内容。");
        const from = start.value ? new Date(start.value).getTime() : Date.now();
        const until = end.value ? new Date(end.value).getTime() : null;
        if (start.value && from <= Date.now())
          throw new Error("开始时间请选择未来，或留空表示立即生效。");
        if (until !== null && until <= from)
          throw new Error("结束时间必须晚于开始时间。");
        return {
          action: "save",
          content: { title: title.value.trim(), text: body.value.trim() },
          effective_at: start.value
            ? scheduled && start.value === localTime(revision.effective_at)
              ? revision.effective_at
              : new Date(start.value).toISOString()
            : null,
          effective_until: end.value
            ? end.value === localTime(revision?.effective_until)
              ? revision.effective_until
              : new Date(end.value).toISOString()
            : null,
          replace_revision: scheduled ? revision.id : null,
          reactivate: !!item?.stopped_at,
        };
      },
      data.context.permissions.some(
        (p) => p.action === permission && p.decision.status === "autonomous"
      )
        ? `保存${noun}`
        : "提交确认"
    );
    title.focus();
  }
  panel.refreshWorkspace = () => {
    if (!editor && !working) refresh();
  };
  content.append(status, button("重新读取", refresh));
  load()
    .then(() => {
      if (panel.isConnected) draw();
    })
    .catch((e) => {
      status.textContent = e.message;
    });
  return panel;
}
