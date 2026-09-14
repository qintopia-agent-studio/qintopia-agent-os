// Additional organization/ledger controls reuse the existing preview/save transaction.
let selectedPosition = null,
  organizationSetup = false;
const statusName = (s) =>
  ({ active: "使用中", draft: "草稿 / 待核验", retired: "已停用" })[s] || s;
function prepareOrganizationState() {
  state.organization ||= { positions: [], ledger: [], audiences: [], history: [] };
  const records = state.organization.ledger;
  state.people.forEach((p) => {
    const entry = records.find((x) => x.kind === "person" && x.object_ref === p.id);
    p.label = entry
      ? entry.nickname
        ? `${entry.label}（${entry.nickname}）`
        : entry.label
      : p.label;
    p.status = entry?.status || "active";
  });
  const duplicates = new Map();
  state.people.forEach((p) =>
    duplicates.set(p.label, (duplicates.get(p.label) || 0) + 1)
  );
  state.people.forEach((p, index) => {
    if (duplicates.get(p.label) > 1) {
      const entry = records.find((x) => x.kind === "person" && x.object_ref === p.id);
      p.label += ` · ${entry?.description || "已核验人员"} · 清单第 ${index + 1} 位`;
    }
  });
  if (!organizationSetup) setupOrganization();
}
function editor(title) {
  $("organizationEditor")?.remove();
  const section = el("section");
  section.id = "organizationEditor";
  section.append(el("h2", title));
  const form = el("form");
  section.append(form);
  const current = [...document.querySelectorAll("[data-view]")].find(
    (x) => !x.hidden && x.dataset.view !== "ledger"
  );
  (current || document.querySelector("main")).append(section);
  form.addEventListener("input", discardPreview);
  return form;
}
function field(form, key, title, items, value = "", required = true) {
  const label = el("label", title),
    input = el(items ? "select" : "input");
  input.id = "org-" + key;
  input.name = key;
  input.required = required;
  label.htmlFor = input.id;
  if (items) {
    input.add(new Option("请选择", ""));
    items.forEach((i) => input.add(new Option(i.label, i.id)));
  }
  input.value = value || "";
  form.append(label, input);
  return input;
}
function submitEditor(form, handler) {
  const bar = el("div", undefined, "buttons"),
    submit = el("button", "预览变更", "primary");
  submit.type = "submit";
  bar.append(
    submit,
    button("收起", () => {
      form.parentElement.remove();
      discardPreview();
    })
  );
  form.append(bar);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    handler(new FormData(form));
  });
  form.querySelector("input,select")?.focus();
}
function openPosition(position = null) {
  showPage("overview");
  const form = editor(position ? "编辑组织岗位" : "设置组织岗位");
  field(form, "label", "组织中显示的岗位名称", null, position?.label);
  field(
    form,
    "role",
    "使用哪个岗位职责定义",
    state.roles.filter(active),
    position?.role_id
  );
  field(
    form,
    "scope",
    "岗位负责的工作范围",
    state.scopes.filter(active),
    position?.scope_id
  );
  field(
    form,
    "parent",
    "上级岗位（可空）",
    state.organization.positions.filter((x) => active(x) && x.id !== position?.id),
    position?.parent_id,
    false
  );
  field(form, "description", "岗位责任说明", null, position?.description);
  field(
    form,
    "status",
    "保存方式",
    [
      { id: "active", label: "启用岗位定义（不授予权限）" },
      { id: "draft", label: "仅保存草稿" },
    ],
    position?.status || "active"
  );
  submitEditor(form, (d) =>
    preview(
      {
        kind: "save_position",
        id: position?.id || null,
        role: d.get("role"),
        scope: d.get("scope"),
        parent: d.get("parent") || null,
        label: d.get("label"),
        description: d.get("description"),
        draft: d.get("status") === "draft",
      },
      `设置岗位：${d.get("label")}\n范围：${labelOf("scopes", d.get("scope"))}\n上级关系不自动授予权限；人员任职另行安排。`
    )
  );
}
function renderOrganization() {
  $("newPosition").disabled = !state.catalog_admin;
  const positions = state.organization.positions;
  $("orgTree").replaceChildren(el("h2", "秦托邦"));
  const build = (parent, depth, seen = new Set()) => {
    for (const pos of positions.filter(
      (x) =>
        (x.parent_id || null) === parent ||
        (parent === null && x.parent_id && !positions.some((p) => p.id === x.parent_id))
    )) {
      if (seen.has(pos.id)) continue;
      seen.add(pos.id);
      const count = state.relations.filter(
        (r) =>
          r.role === pos.role_id && r.scope === pos.scope_id && r.status === "active"
      ).length;
      const row = button(
        `${pos.label} · ${pos.status !== "active" ? statusName(pos.status) : count ? "已任职" : "空缺"}`,
        () => {
          selectedPosition = pos.id;
          renderOrganization();
        },
        "org-node"
      );
      row.style.marginInlineStart = `${Math.min(depth, 4) * 12}px`;
      row.setAttribute("aria-pressed", String(selectedPosition === pos.id));
      $("orgTree").append(row);
      build(pos.id, depth + 1, seen);
    }
  };
  build(null, 0);
  if (!positions.length)
    $("orgTree").append(
      el("p", "尚未设置岗位上下级。现有任职和权限已保留，可先登记岗位归属。")
    );
  const pos = positions.find((x) => x.id === selectedPosition) || positions[0];
  $("orgDetail").replaceChildren(el("h2", pos?.label || "选择岗位，查看工作安排"));
  if (pos) {
    $("orgDetail").append(
      el("p", pos.description),
      el(
        "p",
        `由谁管理：${positions.find((x) => x.id === pos.parent_id)?.label || "秦托邦 / 未设上级岗位"}`
      ),
      el("p", `工作范围：${labelOf("scopes", pos.scope_id)}`)
    );
    const connections = state.relations.filter(
      (r) => r.role === pos.role_id && r.scope === pos.scope_id && r.status === "active"
    );
    if (!connections.length)
      $("orgDetail").append(el("p", "当前空缺。岗位存在不代表有人任职或拥有权限。"));
    connections.forEach((r) => {
      const card = el("article", undefined, "permission-card");
      card.append(
        el("h3", `${labelOf("people", r.person)} ↔ ${text(r.agent)}`),
        el("p", dutyLabel(r) + "：" + r.responsibility)
      );
      const a = state.organization.audiences.find(
        (x) => x.collaboration === r.id
      )?.configuration;
      card.append(
        el("p", a ? audienceSummary(a) : "面对对象尚未设置；真实联系入口未接通。")
      );
      state.grants
        .filter((g) => g.collaboration === r.id && g.revocable)
        .forEach((g) => {
          card.append(
            el(
              "p",
              `${text(g.action)}：${modeNames[g.mode]}${g.reviewer ? " · " + labelOf("people", g.reviewer) : ""}${!g.effective ? "（当前不生效）" : ""}`
            )
          );
          if (g.management_envelope)
            card.append(
              el(
                "p",
                `可安排的智能体：${g.management_envelope.agents.map(text).join("、")}；管理领域：${g.management_envelope.domains.map(text).join("、")}；可授予的决定权：${g.management_envelope.actions.map(text).join("、")}`
              )
            );
        });
      if (!r.immutable)
        card.append(
          button("调整任职与权限", () => editRelation(r)),
          button("配置面对对象", () => openAudience(r)),
          button(
            "结束这项任职",
            () =>
              preview(
                { kind: "end_appointment", appointment: r.appointment },
                `结束 ${labelOf("people", r.person)} 的 ${pos.label} 任职。立即收回同一任职的全部连接权限，保留历史，不等待交接任务。`
              ),
            "danger"
          )
        );
      $("orgDetail").append(card);
    });
    if (state.catalog_admin) {
      if (active(pos))
        $("orgDetail").append(
          button("编辑岗位归属", () => openPosition(pos)),
          button("安排人员任职", () =>
            startRelation({ role: pos.role_id, scope: pos.scope_id })
          )
        );
      lifecycleButtons($("orgDetail"), "position", pos);
    }
  }
  renderLedgers();
  $("orgHistory").replaceChildren();
  state.organization.history.forEach((h) => {
    const record = el("details"),
      change = h.change || {};
    record.append(
      el(
        "summary",
        `${new Date(h.created_at).toLocaleString()} · ${{ assign: "任职与权限", save_position: "岗位归属", save_ledger: "台账登记", lifecycle: "停用 / 恢复", set_audience: "面对对象", end_appointment: "结束任职", end_collaboration: "解除连接" }[change.kind] || "配置变更"} · ${change.after?.label || change.before?.label || "工作配置"}`
      )
    );
    for (const [title, item] of [
      ["变更前", change.before],
      ["变更后", change.after],
    ]) {
      if (item)
        record.append(
          el(
            "p",
            `${title}：${item.label || "联系配置"} · ${item.description || ""} ${item.status ? statusName(item.status) : ""}`
          )
        );
      if (item?.configuration)
        record.append(el("p", audienceSummary(item.configuration)));
    }
    if (change.ended_connections !== undefined)
      record.append(el("p", `结束连接：${change.ended_connections}；恢复授权：0`));
    if (!change.before && !change.after)
      record.append(el("p", "此项旧记录保留操作与任职历史，没有台账字段快照。"));
    $("orgHistory").append(record);
  });
}
function openLedger(kind, entry = null) {
  showPage({ person: "people", agent: "agents", group: "groups" }[kind]);
  const form = editor(
    (entry ? "编辑" : "登记") + { person: "人员", agent: "智能体", group: "群" }[kind]
  );
  if (!entry) {
    const choices =
      kind === "person"
        ? state.people.filter(active)
        : kind === "group"
          ? state.groups
          : state.agents.map((id) => ({ id, label: text(id) }));
    const ref = field(
      form,
      "reference",
      kind === "person"
        ? "查找并关联已有人员（同名请核对说明；留空新建待确认人员）"
        : kind === "agent"
          ? "选择已有智能体（留空登记待接入智能体）"
          : "选择已接入且可访问的群",
      choices,
      "",
      kind === "group"
    );
    const search = field(form, "search", "搜索清单", null, "", false);
    search.type = "search";
    search.addEventListener("input", () => {
      const value = ref.value;
      ref.replaceChildren(new Option("请选择", ""));
      choices
        .filter((x) => x.label.includes(search.value))
        .forEach((x) => ref.add(new Option(x.label, x.id)));
      ref.value = value;
    });
  }
  field(form, "label", "显示名称", null, entry?.label);
  field(form, "nickname", "昵称 / 别名（可空）", null, entry?.nickname, false);
  field(
    form,
    "description",
    kind === "person" ? "人员辨识说明（勿填私密资料）" : "业务职责说明",
    null,
    entry?.description
  );
  field(
    form,
    "scope",
    "业务归属范围（可空）",
    state.scopes.filter(active),
    entry?.scope_id,
    false
  );
  field(
    form,
    "owner",
    "维护责任人（可空）",
    state.people.filter(active),
    entry?.owner_id,
    false
  );
  field(
    form,
    "status",
    "保存方式",
    [
      { id: "active", label: "登记使用（新人员仍须核验身份）" },
      { id: "draft", label: "误建可删除的未使用草稿" },
    ],
    entry?.status || "active"
  );
  submitEditor(form, (d) =>
    preview(
      {
        kind: "save_ledger",
        id: entry?.id || null,
        object: kind,
        reference: entry?.object_ref || d.get("reference") || null,
        label: d.get("label"),
        nickname: d.get("nickname"),
        description: d.get("description"),
        scope: d.get("scope") || null,
        owner: d.get("owner") || null,
        draft: d.get("status") === "draft",
      },
      `登记：${d.get("label")}\n说明：${d.get("description")}\n不会自动任命、关联未知账号、创建 runtime 或授予权限。`
    )
  );
}
function lifecycleButtons(card, object, item) {
  if (!state.catalog_admin) return;
  const operation = item.status === "retired" ? "restore" : "retire";
  card.append(
    button(
      operation === "restore" ? "恢复台账" : "停用并查看影响",
      () =>
        preview(
          { kind: "lifecycle", object, id: item.id, operation },
          `${operation === "restore" ? "恢复" : "停用"}：${item.label}\n${operation === "restore" ? "只恢复台账可用性，不恢复旧任职、授权、群绑定或任务。" : "受影响的当前连接会立即结束并收权；岗位有下级时须先调整下级归属。"}`
        ),
      operation === "retire" ? "danger" : ""
    )
  );
  if (item.status === "draft")
    card.append(
      button(
        "删除误建草稿",
        () =>
          preview(
            { kind: "lifecycle", object, id: item.id, operation: "delete" },
            `删除未使用的误建草稿：${item.label}。已有历史引用时拒绝删除。`
          ),
        "danger"
      )
    );
}
function renderLedgers() {
  for (const [kind, page, target] of [
    ["person", "people", "personLedger"],
    ["agent", "agents", "agentLedger"],
    ["group", "groups", "groupLedger"],
  ]) {
    if (!$(target)) {
      const section = document.querySelector(`[data-view="${page}"]`),
        area = el("section");
      area.id = target;
      section.prepend(area);
    }
    const area = $(target);
    area.replaceChildren();
    if (kind !== "group")
      area.append(el("h2", { person: "人员登记", agent: "智能体登记" }[kind]));
    if (state.catalog_admin && kind !== "group")
      area.append(
        button("＋ 登记" + { person: "人员", agent: "智能体", group: "群" }[kind], () =>
          openLedger(kind)
        )
      );
    const search = el("input");
    search.type = "search";
    search.placeholder = "按名称或说明筛选台账";
    search.setAttribute(
      "aria-label",
      { person: "搜索人员台账", agent: "搜索智能体台账", group: "搜索群台账" }[kind]
    );
    area.append(search);
    const cards = [];
    const entries = state.organization.ledger.filter((x) => x.kind === kind);
    const source =
      kind === "person"
        ? state.people
        : kind === "group"
          ? state.groups
          : state.agents.map((id) => ({ id, label: text(id) }));
    source
      .filter((s) => !entries.some((e) => e.object_ref === s.id))
      .forEach((s) =>
        entries.push({
          id: null,
          kind,
          object_ref: s.id,
          label: s.label,
          nickname: "",
          description: "沿用现有清单；可补充业务归属与维护信息。",
          status: s.status || "active",
          verified: true,
        })
      );
    entries.forEach((entry) => {
      const card = el("article", undefined, "ledger-card");
      card.append(
        el("h3", entry.label + (entry.nickname ? `（${entry.nickname}）` : "")),
        el("span", statusName(entry.status), "tag"),
        el("p", entry.description),
        el(
          "p",
          `归属：${entry.scope_id ? labelOf("scopes", entry.scope_id) : "未设置"} · 维护：${entry.owner_id ? labelOf("people", entry.owner_id) : "未设置"}`
        )
      );
      card.append(
        el(
          "p",
          kind === "agent"
            ? `${entry.verified ? "仓库清单已登记" : "待登记 runtime 清单"} · 实际运行接入未验证`
            : entry.verified
              ? "来源身份已核验"
              : "人员身份待核验；尚不可任命"
        )
      );
      if (state.catalog_admin && entry.status !== "retired")
        card.append(button("编辑登记", () => openLedger(kind, entry)));
      if (entry.id) lifecycleButtons(card, "ledger", entry);
      if (kind === "person" || kind === "agent") {
        const rows = state.relations.filter(
          (r) =>
            (kind === "person" ? r.person : r.agent) === entry.object_ref &&
            r.status === "active"
        );
        rows.forEach((r) =>
          card.append(
            el("p", relationSummary(r)),
            button("查看任职与权限", () =>
              r.immutable ? browse("filterPerson", r.person) : editRelation(r)
            )
          )
        );
        if (active(entry) && entry.verified)
          card.append(
            button("安排工作", () => startRelation({ [kind]: entry.object_ref }))
          );
      }
      cards.push(card);
      area.append(card);
    });
    search.addEventListener("input", () =>
      cards.forEach((card) => (card.hidden = !card.textContent.includes(search.value)))
    );
  }
}
function audienceSummary(a) {
  return `公开咨询：${a.open_reception ? "接待所有主动联系者" : "未开启"}。群聊：${a.groups.map((id) => labelOf("groups", id)).join("、") || "无"}；具体人员：${a.people.map((id) => labelOf("people", id)).join("、") || "无"}；动态对象：${{ none: "未选择", current: "本范围在住", past: "本范围过往", all: "本范围在住及过往" }[a.residents]}。回复：${modeNames[a.reply]}；主动联系：${modeNames[a.proactive]}。内容：${a.topics}`;
}
function openAudience(relation) {
  showPage("config");
  const a = state.organization.audiences.find(
    (x) => x.collaboration === relation.id
  )?.configuration;
  const form = editor(
    `面对对象 · ${labelOf("people", relation.person)} ↔ ${text(relation.agent)} · ${labelOf("scopes", relation.scope)}`
  );
  const groupBox = el("div");
  groupBox.id = "audienceGroups";
  form.append(el("h3", "可以服务哪些群"), groupBox);
  checks(
    "audienceGroups",
    state.groups.filter((g) =>
      state.bindings.some((b) => b.scope === relation.scope && b.conversation === g.id)
    ),
    a?.groups
  );
  const personBox = el("div");
  personBox.id = "audiencePeople";
  form.append(el("h3", "具体人员（可多选）"), personBox);
  checks("audiencePeople", state.people.filter(active), a?.people);
  field(
    form,
    "residents",
    "按事项选择的动态人员范围",
    [
      { id: "none", label: "不选择动态对象" },
      { id: "current", label: "本范围在住" },
      { id: "past", label: "本范围过往" },
      { id: "all", label: "本范围在住和过往全部" },
    ],
    a?.residents || "none"
  );
  field(
    form,
    "openReception",
    "公开咨询与信息接待",
    [
      { id: "yes", label: "接待所有主动联系者（仅公开咨询，不要求登记或入住）" },
      { id: "no", label: "此条连接不承担开放接待" },
    ],
    a?.open_reception === false ? "no" : "yes"
  );
  form.append(
    el(
      "p",
      "这里只设可用范围，每次事项再选具体对象。实际在住 / 过往身份仍须 PMS 核验，不能凭群成员或姓名推断。",
      "muted"
    )
  );
  const modes = Object.entries(modeNames).map(([id, label]) => ({ id, label }));
  field(form, "reply", "收到私聊后回复 / 交换信息", modes, a?.reply || "denied");
  field(
    form,
    "proactive",
    "主动联系（仍需这条连接的发布授权）",
    modes,
    a?.proactive || "denied"
  );
  field(
    form,
    "reviewer",
    "需要确认时找谁",
    state.people.filter((p) => active(p) && p.id !== relation.person),
    a?.reviewer,
    false
  );
  field(form, "topics", "私聊内容与工作边界", null, a?.topics);
  field(
    form,
    "visibility",
    "信息可见性",
    [
      { id: "general", label: "范围内一般服务信息" },
      { id: "service_private", label: "仅事项必要的服务私密信息（需消费端核验）" },
    ],
    a?.visibility || "general"
  );
  submitEditor(form, (d) => {
    const audience = {
      open_reception: d.get("openReception") === "yes",
      groups: selected("audienceGroups"),
      people: selected("audiencePeople"),
      residents: d.get("residents"),
      reply: d.get("reply"),
      proactive: d.get("proactive"),
      reviewer: d.get("reviewer") || null,
      topics: d.get("topics"),
      visibility: d.get("visibility"),
    };
    preview(
      { kind: "set_audience", collaboration: relation.id, audience },
      audienceSummary(audience) + "\n不会发送消息或开放整个人员资料库。"
    );
  });
}
function setupOrganization() {
  organizationSetup = true;
  $("newPosition").addEventListener("click", () => openPosition());
  $("registerGroup").addEventListener("click", () => openLedger("group"));
  const originalRender = render;
  render = function () {
    originalRender();
    for (const r of state.relations.filter(
      (r) => r.status === "active" && !r.immutable
    )) {
      // Keep contact configuration discoverable without mixing it into role definitions.
      const card = el("article", undefined, "contact-row");
      card.append(
        el("p", relationSummary(r)),
        button("配置面对对象与私聊", () => openAudience(r))
      );
      $("contactConnections").append(card);
    }
  };
  const section = el("section");
  section.append(el("h2", "面对对象与联系边界"));
  const list = el("div");
  list.id = "contactConnections";
  section.append(list);
  document.querySelector('[data-view="config"]').append(section);
  const wrapped = render;
  render = function () {
    list.replaceChildren();
    wrapped();
  };
}
