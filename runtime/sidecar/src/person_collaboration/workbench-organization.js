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
}
function renderPersonalWork() {
  const target = $("overview");
  target.replaceChildren();
  const own = state.relations.filter(
    (r) => r.person === state.actor_person && ["active", "scheduled"].includes(r.status)
  );
  const name = state.actor_person ? personName(state.actor_person) : "当前账号";
  target.append(titleRow("我的工作", `${name}，这里只显示与你有关的工作。`));
  if (!own.length) {
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
      work.append(personalRulePanel(relation.scope));
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
    if (grants.length) work.append(details("查看我能决定的事项", summary, list));
    else work.append(sub("当前没有可用的决定权，请联系负责人核对安排。"));
    const more = details("查看任期与工作依据", sub(termSummary(relation)));
    more.addEventListener("toggle", () => {
      if (more.open && !more.dataset.loaded) {
        more.dataset.loaded = "true";
        more.append(constraintsPanel(relation.scope));
      }
    });
    work.append(more);
    target.append(work);
  }
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

// Business settings use the existing rule service, never organization edit authority.
function personalRulePanel(scope) {
  const panel = box("本栋约定"),
    content = el("div");
  panel.append(sub("你可以在已有权限内修改本栋规则，或新增一项工作约定。"), content);
  explainHeading(
    panel,
    "本栋约定",
    "只影响当前楼栋。保存后由二花读取；上层共同约束仍适用。保存不会自动通知群，也不会改变你的岗位和权限。"
  );
  if (!state.local_dialogue_available) {
    content.append(sub("当前本地业务入口未启用，暂时无法读取和保存约定。"));
    return panel;
  }
  const prefix = `work-rule-${crypto.randomUUID()}`;
  let context,
    editor = null;
  const ruleTitle = (rule) =>
    rule.content?.title || (rule.key === "kitchen" ? "厨房使用" : "本栋工作约定");
  const editable = (rule) =>
    rule.scope === scope &&
    rule.kind === "rule" &&
    !rule.shared &&
    rule.content &&
    typeof rule.content.text === "string" &&
    Object.keys(rule.content).every((key) => ["text", "title"].includes(key));
  const status = el("p", "正在读取本栋约定……", "qo-sub");
  status.setAttribute("role", "status");
  content.append(status);
  async function load() {
    context = await api("/api/foundation/context", { scope });
  }
  function draw() {
    content.replaceChildren(status);
    status.textContent = "";
    const rules = context.knowledge.filter(editable);
    for (const rule of rules) {
      const row = el("article", undefined, "qo-scope-row");
      row.append(
        el("h4", ruleTitle(rule)),
        el("p", rule.content.text),
        sub(
          `当前生效 · ${displayTime(rule.effective_at)}${rule.effective_until ? ` · 截至 ${displayTime(rule.effective_until)}` : ""}`
        )
      );
      content.append(row);
    }
    if (!rules.length)
      content.append(sub("目前没有可直接编辑的文本约定，可以从新增一项开始。"));
    const later = context.later_revisions.filter(editable);
    for (const rule of later)
      content.append(
        sub(
          `待生效：${ruleTitle(rule)} · ${displayTime(rule.effective_at)} · ${rule.content.text}`
        )
      );
    const rights = context.permissions.filter((p) => p.action === "change_rules");
    const canWrite = rights.some((p) =>
      ["autonomous", "confirmation_required"].includes(p.decision.status)
    );
    if (!canWrite) {
      content.append(sub("当前没有修改本栋约定的决定权，请联系负责人核对授权。"));
      return;
    }
    content.append(
      sub(
        rights.some((p) => p.decision.status === "autonomous")
          ? "你可自主保存本栋约定。"
          : "你可以提出修改，需授权中指定的人确认后生效。"
      )
    );
    const bar = actions(
      ...rules.map((rule) => button(`修改${ruleTitle(rule)}`, () => edit(rule))),
      button("新增约定", () => edit(null), "qo-primary")
    );
    content.append(bar);
    const link = el("a", "向二花交代其他事项", "qo-work-link");
    link.href = "/foundation";
    content.append(
      actions(link),
      sub("具体事项和交流偏好也可以告诉二花；当前对话入口仅支持列出的本地例句。")
    );
  }
  function edit(rule) {
    editor?.remove();
    const form = el("form", undefined, "qo-work-editor");
    editor = form;
    const key = rule?.key || `local_${crypto.randomUUID()}`;
    let until = rule?.effective_until || null;
    let version =
      context.knowledge_items.find((r) => r.key === key)?.latest_version || 0;
    form.append(
      el("h4", rule ? `修改${ruleTitle(rule)}` : "新增本栋约定"),
      sub(`仅用于${labelOf("scopes", scope)}，不改变其他楼栋。`)
    );
    const title = inputField(
      form,
      `${prefix}-title`,
      "约定名称",
      rule ? ruleTitle(rule) : "",
      "text",
      true
    );
    title.placeholder = "例如：公共空间使用";
    const body = inputField(
      form,
      `${prefix}-body`,
      "具体约定",
      rule?.content.text || "",
      "textarea",
      true
    );
    body.placeholder = "写清楚你希望如何安排，二花会读取保存后的约定。";
    const time = inputField(
      form,
      `${prefix}-time`,
      "开始生效时间（留空即保存后生效）",
      "",
      "datetime-local"
    );
    fieldExplanation(
      time,
      "生效时间",
      "未来生效前仍使用当前约定。已有待生效版本不会被这次修改删除，请先核对上面的待生效内容。若需指定人确认，批准前不会生效。"
    );
    const current = sub(rule ? `本次依据：${rule.content.text}` : "这是新增约定。");
    if (rule?.effective_until)
      current.textContent += ` 原截止时间 ${displayTime(rule.effective_until)} 保留。`;
    const feedback = el("p", "", "qo-sub");
    feedback.setAttribute("role", "status");
    const save = el("button", "保存本栋约定", "qo-primary");
    save.type = "submit";
    const reload = button("重新读取并核对（保留草稿）", async () => {
      panel.inert = true;
      try {
        await load();
        version =
          context.knowledge_items.find((r) => r.key === key)?.latest_version || 0;
        const latest = context.knowledge.find(
          (r) => r.key === key && r.scope === scope
        );
        current.textContent = latest
          ? `当前已生效：${latest.content.text}`
          : "当前没有此项已生效的约定。";
        until = latest?.effective_until || null;
        if (until) current.textContent += ` 原截止时间 ${displayTime(until)} 保留。`;
        const pending = context.later_revisions.filter((r) => r.key === key);
        if (pending.length)
          current.textContent +=
            " 待生效：" +
            pending
              .map((r) => `${displayTime(r.effective_at)}：${r.content.text}`)
              .join("；");
        save.disabled = !context.permissions.some(
          (p) =>
            p.action === "change_rules" &&
            ["autonomous", "confirmation_required"].includes(p.decision.status)
        );
        feedback.textContent = save.disabled
          ? "修改权限已收回，草稿保留供你核对。"
          : "草稿已保留。请与当前约定核对；如已保存，无需再次提交。";
      } catch (error) {
        feedback.textContent = error.message;
      } finally {
        panel.inert = false;
      }
    });
    reload.hidden = true;
    form.append(
      current,
      actions(
        save,
        button("取消", () => {
          form.remove();
          editor = null;
        })
      ),
      feedback,
      reload
    );
    form.addEventListener("submit", async (event) => {
      event.preventDefault();
      if (busy || save.disabled || panel.inert) return;
      if (!title.value.trim() || !body.value.trim()) {
        feedback.textContent = "请填写约定名称和具体内容。";
        return;
      }
      if (time.value && new Date(time.value).getTime() <= Date.now()) {
        feedback.textContent = "请选择未来时间，或留空表示保存后生效。";
        return;
      }
      if (
        until &&
        (time.value ? new Date(time.value).getTime() : Date.now()) >=
          new Date(until).getTime()
      ) {
        feedback.textContent = "开始时间须早于这项约定原有的截止时间。";
        return;
      }
      panel.inert = true;
      let saved = false;
      try {
        const result = await api("/api/foundation/rule", {
          scope,
          operation_id: crypto.randomUUID(),
          expected_version: version,
          key,
          kind: "rule",
          shared: false,
          content: { title: title.value.trim(), text: body.value.trim() },
          effective_at: time.value ? new Date(time.value).toISOString() : null,
          effective_until: until,
        });
        saved = true;
        await load();
        draw();
        status.textContent =
          result.status === "saved"
            ? `已保存并重新读取。${time.value ? `${displayTime(result.knowledge.effective_at)}开始生效。` : "现在开始生效。"}未发送群通知。`
            : "修改已提交，等待指定人员确认；当前约定保持不变。";
        editor = null;
      } catch (error) {
        save.disabled = true;
        reload.hidden = false;
        feedback.textContent = saved
          ? "已取得保存回执，但回读失败。请重新读取核对，不要重复提交。"
          : error.message;
      } finally {
        panel.inert = false;
      }
    });
    content.append(form);
    title.focus();
  }
  load()
    .then(() => {
      if (panel.isConnected) draw();
    })
    .catch((error) => {
      status.textContent = error.message;
      content.append(
        button("重新读取本栋约定", () => {
          status.textContent = "正在重新读取……";
          load()
            .then(draw)
            .catch((e) => {
              status.textContent = e.message;
            });
        })
      );
    });
  return panel;
}
