// Each visible connection binds one person, role, duty, Agent and scope.
const permissionHelp = {
  confirm_knowledge: [
    "把待确认的信息确认为智能体可以使用的正式知识，或纠正、否定旧知识。",
    "例如：确认公共空间的使用安排；更新过时的生活指引。",
    "人人可提供信息；能确认知识，不等于可以公开门禁或私人资料。",
  ],
  train: [
    "通过对话纠正智能体的回答、语气、提醒方式和工作方法。",
    "例如：指导二花怎样向本栋居民介绍公共空间安排。",
    "只改进本职责范围内的服务，不因此取得修改社区原则、技术代码或其他楼栋服务的权限。",
  ],
  change_rules: [
    "在秦托邦的文化和责任边界内，决定本范围的运营约定。",
    "例如：舍长与居民商定本栋厨房的使用安排。",
    "不能修改社区统一原则，也不能据此批准超出岗位的收费或资金处置。",
  ],
  review: [
    "检查智能体准备发给居民或合作人的内容，确认是否准确、合适。",
    "例如：审核一则本栋通知，或一份欢迎介绍。",
    "审核通过不等于已经发送；发送仍需符合对象、渠道和流程条件。",
  ],
  publish: [
    "允许智能体在这条连接的范围内，按已确认的约定主动向群聊或个人发送内容。",
    "例如：发送本栋通知，或跟进居民提出的服务问题。",
    "不是任意群发；仍需核对内容审核、联系意愿、可见性和具体流程条件。",
  ],
  designate: [
    "为职责范围内的业务指定负责核对、确认的人。",
    "例如：指定本栋物资记录由谁核实。",
    "不会自动给该人训练权、系统管理权或全社区财务权限。",
  ],
  identity: [
    "核对已有人员与渠道账号的对应关系，处理匹配错误或撤销。",
    "例如：核实两个同名账号中哪个属于这位责任人。",
    "不能仅凭姓名认定身份，也不因此取得私密资料访问权。",
  ],
  manage: [
    "安排谁担任岗位、承担哪项职责，并在获授权范围内为连接授予或撤销权限。",
    "例如：社区负责人安排某人成为一栋舍长，与二花共同服务一栋。",
    "可授给别人什么要单独限定；管理权不等于本人拥有所有业务决定权。",
  ],
  technical_support: [
    "帮助责任人和智能体解决使用、训练效果及工具故障问题。",
    "例如：协助查明已确认知识为什么没有被正确引用。",
    "不代替业务负责人裁定事项，不自动开放私密资料、生产部署或权限扩大。",
  ],
};
let currentPage = "overview";
let ledgerPage = "people";
function el(tag, content, className) {
  const node = document.createElement(tag);
  if (content !== undefined) node.textContent = content;
  if (className) node.className = className;
  return node;
}
function button(label, handler, className) {
  const b = el("button", label, className);
  b.type = "button";
  b.addEventListener("click", handler);
  return b;
}
function showPage(page) {
  if (currentPage !== page) discardPreview();
  currentPage = page;
  const catalogs = [
    "people",
    "agents",
    "groups",
    "roles",
    "duties",
    "scopes",
    "history",
  ];
  if (catalogs.includes(page)) ledgerPage = page;
  const ledger = page === "ledger" || catalogs.includes(page);
  document
    .querySelectorAll("[data-view]")
    .forEach(
      (n) =>
        (n.hidden = ledger
          ? !["ledger", ledgerPage].includes(n.dataset.view)
          : n.dataset.view !== page)
    );
  document.querySelectorAll("nav [data-page]").forEach((n) => {
    if (
      n.dataset.page === (ledger ? "ledger" : page === "assignment" ? "config" : page)
    )
      n.setAttribute("aria-current", "page");
    else n.removeAttribute("aria-current");
  });
  window.scrollTo({ top: 0 });
}
function dutyChecks(id, actions, chosen = []) {
  $(id).replaceChildren();
  actions.forEach((action) => {
    const label = el("label", undefined, "duty"),
      input = el("input"),
      body = el("span");
    input.type = "checkbox";
    input.value = action;
    input.checked = chosen.includes(action);
    body.append(el("strong", text(action)), el("small", permissionHelp[action]?.[0]));
    label.append(input, body);
    $(id).append(label);
  });
}
function roleDuties(presetDuty = prefilledDuty) {
  const role = state.roles.find((r) => r.id === $("role").value);
  const choices = state.duties.filter(
    (d) => active(d) && (role?.duty_ids || []).includes(d.id)
  );
  optionList("duty", choices, "请选择职责");
  $("duty").value =
    presetDuty && choices.some((d) => d.id === presetDuty)
      ? presetDuty
      : choices.length === 1
        ? choices[0].id
        : "";
  $("roleContext").textContent = role
    ? `${role.label}的职责来自岗位设置。岗位不自动赋权；每项权限由这条连接单独决定。`
    : "请先选定岗位与职责。";
  permissionControls();
  dutyContext();
  scopeContext();
}
function dutyContext() {
  const duty = state.duties.find((d) => d.id === $("duty").value);
  $("dutyContext").textContent = duty
    ? `${duty.label}：${duty.description}`
    : $("role").value
      ? "请选择这次承担的职责。若没有适用职责，可在“岗位”中关联已有职责，或先在“职责”中新增。"
      : "先选岗位，再选择它所关联的职责。";
}
function scopeContext() {
  const groups = state.bindings
    .filter((b) => b.scope === $("scope").value)
    .map((b) => labelOf("groups", b.conversation));
  $("scopeContext").textContent = $("scope").value
    ? "这个范围对应的群聊：" +
      (groups.join("、") || "尚未绑定群聊，可在“工作范围”中设置。")
    : "选择工作范围后，可查看它对应的群聊。";
}
function permissionControls(grants = []) {
  const duty = state.duties.find((d) => d.id === $("duty").value);
  $("permissions").replaceChildren();
  for (const action of duty?.available_actions || []) {
    const grant = grants.find((g) => g.action === action);
    const card = el("article", undefined, "permission-card");
    card.dataset.action = action;
    const copy = el("div", undefined, "permission-copy");
    copy.append(
      el("h3", text(action)),
      el("p", permissionHelp[action]?.[0]),
      el("p", permissionHelp[action]?.[1], "example")
    );
    const more = el("details");
    more.append(el("summary", "这项权限的边界"), el("p", permissionHelp[action]?.[2]));
    copy.append(more);
    const control = el("div", undefined, "permission-control");
    const modeLabel = el("label", "决定方式"),
      mode = el("select");
    mode.id = "mode-" + action;
    modeLabel.htmlFor = mode.id;
    mode.dataset.mode = "";
    ["denied", "autonomous", "confirmation"].forEach((value) =>
      mode.add(new Option(modeNames[value], value))
    );
    mode.value = grant?.mode || (grant ? "autonomous" : "denied");
    const reviewerWrap = el("div"),
      reviewerLabel = el("label", "由谁确认"),
      reviewer = el("select");
    reviewer.id = "reviewer-" + action;
    reviewerLabel.htmlFor = reviewer.id;
    reviewer.dataset.reviewer = "";
    reviewer.add(new Option("选择有对应自主决定权的人员", ""));
    state.people
      .filter((p) => p.id !== $("person").value)
      .forEach((p) => reviewer.add(new Option(p.label, p.id)));
    reviewer.value = grant?.reviewer || "";
    reviewerWrap.append(
      reviewerLabel,
      reviewer,
      el(
        "p",
        "确认人须在同一智能体、职责与范围内拥有这项自主决定权。保存前会核验。",
        "muted"
      )
    );
    const hint = el("p", undefined, "muted");
    const update = () => {
      reviewerWrap.hidden = mode.value !== "confirmation";
      reviewer.required = !reviewerWrap.hidden;
      card.dataset.mode = mode.value;
      hint.textContent =
        mode.value === "autonomous"
          ? "责任人可在职责范围内作出这类决定。"
          : mode.value === "confirmation"
            ? "责任人可以提出和整理，由指定人员确认后才能继续。"
            : "不能作出这类决定，仍可交流、提供信息和提出建议。";
    };
    mode.addEventListener("change", update);
    update();
    control.append(modeLabel, mode, hint, reviewerWrap);
    if (grant && !grant.effective && mode.value !== "denied")
      control.append(
        el("p", "原配置目前不生效，请检查任职、授权来源或确认人资格。", "warning")
      );
    card.append(copy, control);
    $("permissions").append(card);
  }
  if (!$("permissions").children.length)
    $("permissions").append(
      el(
        "p",
        duty
          ? "这项职责尚未设置可选权限。可以先建立不授予决定权的连接，后续再补充。"
          : "选择职责后，会显示它适用的少量权限类别。",
        "empty"
      )
    );
  updateDelegation();
}
function readPermissions() {
  return [...$("permissions").querySelectorAll("[data-action]")].map((row) => {
    const mode = row.querySelector("[data-mode]").value;
    return {
      action: row.dataset.action,
      mode,
      reviewer:
        mode === "confirmation"
          ? row.querySelector("[data-reviewer]").value || null
          : null,
    };
  });
}
function updateDelegation() {
  $("delegation").hidden = !readPermissions().some(
    (p) => p.action === "manage" && p.mode === "autonomous"
  );
}
function filteredRelations() {
  const q = $("search").value.trim().toLocaleLowerCase();
  return state.relations.filter((r) => {
    const words = [
      labelOf("people", r.person),
      labelOf("roles", r.role),
      dutyLabel(r),
      text(r.agent),
      labelOf("scopes", r.scope),
      ...state.bindings
        .filter((b) => b.scope === r.scope)
        .map((b) => labelOf("groups", b.conversation)),
    ]
      .join(" ")
      .toLocaleLowerCase();
    return (
      words.includes(q) &&
      (!$("filterPerson").value || r.person === $("filterPerson").value) &&
      (!$("filterScope").value || r.scope === $("filterScope").value) &&
      (!$("filterAgent").value || r.agent === $("filterAgent").value) &&
      (!$("filterDuty").value || r.duty === $("filterDuty").value) &&
      ($("filterStatus").value === "all" || r.status === "active")
    );
  });
}
function browse(filter, value) {
  for (const id of [
    "search",
    "filterPerson",
    "filterScope",
    "filterAgent",
    "filterDuty",
  ])
    $(id).value = "";
  $(filter).value = value;
  render();
  showPage("config");
}
function relationSummary(r) {
  return `${labelOf("people", r.person)} · ${labelOf("roles", r.role)} · ${dutyLabel(r)} ↔ ${text(r.agent)} · ${labelOf("scopes", r.scope)}`;
}
function statusTag(r) {
  return el(
    "span",
    r.status === "active" ? "当前连接" : r.status === "expired" ? "已到期" : "已结束",
    r.status === "active" ? "tag" : "tag historical"
  );
}
function showRelationDetail(r) {
  chooseView(false);
  const detail = [...$("network").children].find((n) => n.dataset.relation === r.id);
  if (!detail) return;
  detail.focus({ preventScroll: true });
  window.scrollTo({ top: detail.getBoundingClientRect().top + window.scrollY - 24 });
}
function renderGraph() {
  $("graph").replaceChildren();
  filteredRelations().forEach((r) => {
    const row = el(
      "article",
      undefined,
      "connection-row" + (r.domain === "technical_support" ? " technical" : "")
    );
    row.dataset.relation = r.id;
    const chain = el("div", undefined, "connection-chain");
    const items = [
      ["人", labelOf("people", r.person), () => browse("filterPerson", r.person)],
      [
        "岗位",
        labelOf("roles", r.role),
        () => {
          showPage("roles");
          openRole(state.roles.find((x) => x.id === r.role));
        },
      ],
      [
        "职责",
        dutyLabel(r),
        r.duty ? () => browse("filterDuty", r.duty) : () => showRelationDetail(r),
      ],
      ["智能体", text(r.agent), () => browse("filterAgent", r.agent)],
      ["工作范围", labelOf("scopes", r.scope), () => browse("filterScope", r.scope)],
    ];
    items.forEach(([kind, label, click], index) => {
      const node = button(
        "",
        click,
        "connection-node" + (kind === "智能体" ? " agent" : "")
      );
      node.append(el("small", kind), el("strong", label));
      if (index) {
        const line = el("span", index === 3 ? "↔" : "→", "connection-line");
        line.setAttribute("aria-hidden", "true");
        chain.append(line);
      }
      chain.append(node);
    });
    const footer = el("div", undefined, "connection-footer");
    const context = el("div");
    context.append(statusTag(r));
    if (!r.duty && !r.immutable)
      context.append(el("span", "待关联职责", "tag warning"));
    const granted = state.grants.filter(
      (g) => g.collaboration === r.id && g.effective && g.mode !== "denied"
    );
    const autonomous = granted.filter((g) => !g.mode || g.mode === "autonomous").length;
    const confirmation = granted.filter((g) => g.mode === "confirmation").length;
    context.append(
      el(
        "span",
        r.duty
          ? `配置：自主决定 ${autonomous} 项 · 指定确认 ${confirmation} 项`
          : r.immutable
            ? "初始化管理配置"
            : "旧配置待关联职责",
        "muted"
      )
    );
    footer.append(
      context,
      button("查看连接与权限", () => showRelationDetail(r))
    );
    row.append(chain, footer);
    $("graph").append(row);
  });
  if (!$("graph").children.length)
    $("graph").append(
      el("p", "没有符合条件的已保存连接。可以调整筛选，或建立连接。", "empty")
    );
}
function decisionCheck(r) {
  const box = el("details", undefined, "decision-check");
  box.append(el("summary", "检查这条连接的权限"));
  const duty = state.duties.find((d) => d.id === r.duty);
  const dutyActions = duty?.available_actions || [];
  const recordedActions = state.grants
    .filter((g) => g.collaboration === r.id)
    .map((g) => g.action);
  const actions = [...new Set([...dutyActions, ...recordedActions])];
  if (!actions.length) {
    box.append(
      el(
        "p",
        r.duty
          ? "这项职责尚未设置权限类别，也没有已保存的权限配置，目前没有权限可检查。可先保留职责草稿，后续再补充。"
          : "这条旧连接没有可检查的权限配置，请先关联职责并明确决定权。",
        "muted"
      )
    );
    return box;
  }
  box.append(
    el(
      "p",
      "选择一种决定，核对当前配置会如何判断。这只是检查，不会执行真实动作，也不代表某项业务已获确认。",
      "muted"
    )
  );
  const bar = el("div", undefined, "decision-controls"),
    select = el("select"),
    output = el("p", undefined, "decision-result");
  select.setAttribute("aria-label", "检查哪项权限");
  actions.forEach((action) =>
    select.add(
      new Option(
        text(action) + (r.duty && !dutyActions.includes(action) ? " · 历史配置" : ""),
        action
      )
    )
  );
  output.setAttribute("role", "status");
  const check = button("检查权限", async () => {
    check.disabled = true;
    output.textContent = "正在核对……";
    try {
      const result = await api("/api/decision", {
        collaboration: r.id,
        action: select.value,
      });
      const reasons = {
        permission_not_granted: "当前未授予这项权限。",
        conflicting_permission_settings: "当前存在互相冲突的权限设置，需要先处理冲突。",
        duty_required: "这条连接尚未关联职责，请先补齐职责。",
        eligible_reviewer_required: "需要另一位具有对应自主决定权的确认人。",
        authorization_chain_inactive:
          "本人或上游授权来源已失效，请检查任职、身份与授权。",
        within_granted_boundary: "当前连接中的自主决定权有效。",
        reviewer_authority_inactive:
          "指定确认人已失去同一职责、智能体和范围内的自主决定权。",
        designated_person_must_confirm: "必须由指定人员对具体事项进行确认后才能继续。",
        reviewer_not_authorized: "确认人当前已无对应自主决定权。",
        reviewer_unavailable: "确认人资格已失效，请调整配置。",
        no_effective_grant: "当前没有有效授权。",
        no_grant: "当前未授予这项权限。",
        permission_denied: "这项权限设为未授予。",
        explicit_denial: "这项权限设为未授予。",
        legacy_duty_unassigned: "旧连接尚未关联职责。",
        collaboration_inactive: "连接或任职已结束、到期。",
        grant_inactive: "授权或其来源已失效。",
        autonomous: "当前连接中的自主决定权有效。",
        confirmation_required: "必须先由指定人员对具体事项进行确认。",
      };
      const status =
        result.status === "autonomous"
          ? "可自主决定"
          : result.status === "confirmation_required"
            ? "需要 " + labelOf("people", result.reviewer) + " 确认"
            : "未授予／当前不可决定";
      output.textContent =
        status +
        "。" +
        (reasons[result.reason] ||
          "判断依据包括当前连接、职责范围、授权来源及确认人资格。") +
        " 此结果只解释当前配置，未执行任何动作。";
      output.dataset.status = result.status;
    } catch (error) {
      output.textContent = error.message;
      output.dataset.status = "denied";
    } finally {
      check.disabled = false;
    }
  });
  bar.append(select, check);
  box.append(bar, output);
  return box;
}
function renderList() {
  $("network").replaceChildren();
  filteredRelations().forEach((r) => {
    const card = el(
      "article",
      undefined,
      "relation" + (r.status !== "active" ? " inactive" : "")
    );
    card.tabIndex = -1;
    card.dataset.relation = r.id;
    card.append(el("h3", relationSummary(r)), statusTag(r), el("p", r.responsibility));
    const groups = state.bindings
      .filter((b) => b.scope === r.scope)
      .map((b) => labelOf("groups", b.conversation));
    card.append(
      el(
        "p",
        (groups.join("、") || "尚未绑定群聊") +
          " · " +
          (r.valid_until
            ? new Date(r.valid_until).toLocaleString() + " 截止"
            : "无固定截止时间"),
        "muted"
      )
    );
    if (!r.duty && !r.immutable)
      card.append(
        el(
          "p",
          "旧连接待关联职责。请调整这条连接，补齐职责并明确每项决定权。",
          "warning"
        )
      );
    const grants = state.grants
      .filter((g) => g.collaboration === r.id && g.revocable)
      .sort(
        (a, b) => state.actions.indexOf(a.action) - state.actions.indexOf(b.action)
      );
    const permissionList = el("div", undefined, "saved-permissions");
    grants.forEach((g) => {
      const item = el("div", undefined, "saved-permission"),
        mode = g.mode || "autonomous";
      item.append(
        el("strong", text(g.action)),
        el(
          "span",
          r.duty
            ? modeNames[mode] +
                (g.reviewer ? " · " + labelOf("people", g.reviewer) : "") +
                (!g.effective && mode !== "denied" ? " · 当前不生效" : "")
            : r.immutable
              ? "初始化管理配置，仅用于本地设置"
              : "旧配置待关联职责",
          "muted"
        )
      );
      permissionList.append(item);
    });
    if (!grants.length)
      permissionList.append(el("p", "没有当前可用的决定权。", "muted"));
    card.append(permissionList, decisionCheck(r));
    if (r.immutable)
      card.append(
        el("p", "初始化管理关系：用于本地配置管理，当前页面不支持改写。", "muted")
      );
    else if (r.status === "active") {
      const bar = el("div", undefined, "buttons");
      bar.append(
        button("调整这条连接", () => editRelation(r)),
        button(
          "结束这条连接",
          () =>
            preview(
              { kind: "end_collaboration", collaboration: r.id },
              "结束连接：" +
                relationSummary(r) +
                "\n结束这条连接，其授权以及依赖它的权限随之失效。此人的其他连接保留；若这是该任职最后一条连接，空任职也会结束。"
            ),
          "danger"
        )
      );
      card.append(bar);
    }
    $("network").append(card);
  });
  if (!$("network").children.length)
    $("network").append(el("p", "没有符合条件的连接。", "empty"));
}
function chooseView(graph) {
  $("graph").hidden = !graph;
  $("network").hidden = graph;
  $("graphHint").hidden = !graph;
  $("graphView").setAttribute("aria-pressed", String(graph));
  $("listView").setAttribute("aria-pressed", String(!graph));
}
function render() {
  renderGraph();
  renderList();
}
function directory(id, items, describe) {
  $(id).replaceChildren();
  items.forEach((item) => {
    const card = el("article");
    card.append(el("h3", item.label));
    describe(card, item);
    $(id).append(card);
  });
  if (!items.length) $(id).append(el("p", "当前没有可显示的记录。", "empty"));
}
function setupViews() {
  optionList("filterPerson", state.people, "全部人员");
  optionList("filterScope", state.scopes, "全部范围");
  optionList("filterDuty", state.duties, "全部职责");
  optionList(
    "filterAgent",
    state.agents.map((id) => ({ id, label: text(id) })),
    "全部智能体"
  );
  directory("peopleDirectory", state.people, (card, p) => {
    const relations = state.relations.filter(
      (r) => r.person === p.id && r.status === "active"
    );
    relations.forEach((r) =>
      card.append(
        el(
          "p",
          `${labelOf("roles", r.role)} · ${dutyLabel(r)}\n${text(r.agent)} · ${labelOf("scopes", r.scope)}`
        )
      )
    );
    if (!relations.length) card.append(el("p", "尚未安排当前工作"));
    const bar = el("div", undefined, "buttons");
    bar.append(
      button("查看连接", () => browse("filterPerson", p.id)),
      button("为他安排工作", () => startRelation({ person: p.id }))
    );
    card.append(bar);
    const appointments = [
      ...new Map(
        relations.filter((r) => !r.immutable).map((r) => [r.appointment, r])
      ).values(),
    ];
    if (appointments.length) {
      const details = el("details");
      details.append(
        el("summary", "任职与交接"),
        el(
          "p",
          "结束任职会结束这个岗位与范围下的全部连接；只想停一项工作，请结束对应连接。",
          "muted"
        )
      );
      appointments.forEach((r) =>
        details.append(
          button(
            "结束任职：" +
              labelOf("roles", r.role) +
              " · " +
              labelOf("scopes", r.scope),
            () =>
              preview(
                { kind: "end_appointment", appointment: r.appointment },
                `结束任职：${p.label} · ${labelOf("roles", r.role)} · ${labelOf("scopes", r.scope)}\n该任职下的所有连接和授权将失效，其他岗位任职保留。`
              ),
            "danger"
          )
        )
      );
      card.append(details);
    }
  });
  directory(
    "agentDirectory",
    state.agents.map((id) => ({ id, label: text(id) })),
    (card, a) => {
      const rows = state.relations.filter(
        (r) => r.agent === a.id && r.status === "active"
      );
      rows.forEach((r) =>
        card.append(
          el(
            "p",
            `${labelOf("people", r.person)} · ${dutyLabel(r)} · ${labelOf("scopes", r.scope)}`
          )
        )
      );
      if (!rows.length) card.append(el("p", "尚未建立当前连接"));
      const bar = el("div", undefined, "buttons");
      bar.append(
        button("查看连接", () => browse("filterAgent", a.id)),
        button("安排配合人员", () => startRelation({ agent: a.id }))
      );
      card.append(bar);
    }
  );
}
