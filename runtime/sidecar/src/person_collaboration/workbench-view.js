/* Shared presentation helpers: insert service content only as text. */
const $ = (id) => document.getElementById(id);
const names = {
  default: "管理入口",
  erhua: "二花",
  xiaoman: "小满",
  silaoshi: "四老师",
  wenyuange: "文渊阁",
  guanerye: "关二爷",
  huabaosi: "阿靓",
  anan: "岸岸",
  train: "指导智能体改进服务",
  confirm_knowledge: "确认可用于回答的知识",
  change_rules: "决定职责范围内的运营规则",
  review: "审核准备发给他人的内容",
  publish: "允许按约定主动联系",
  designate: "指定业务确认人",
  identity: "核对人员与渠道账号关联",
  manage: "设置协作人员及下级权限",
  technical_support: "技术支持",
  community_service: "居民服务",
  activity_operations: "活动运营",
  hospitality: "客房服务",
  organization: "组织协作",
};
const modeNames = {
  autonomous: "可自主决定",
  confirmation: "需指定人员确认",
  denied: "未授予",
};
const statusNames = {
  active: "在册",
  draft: "草稿 / 待核验",
  retired: "已停用",
  ended: "已结束",
  expired: "已到期",
  scheduled: "尚未开始",
  confirmed: "已核验",
  pending: "待核验",
  revoked: "已撤销",
};
const text = (key) => names[key] || key;
const active = (x) => !x.status || x.status === "active";
const el = (tag, content, cls) => {
  const e = document.createElement(tag);
  if (content !== undefined) e.textContent = content;
  if (cls) e.className = cls;
  return e;
};
const button = (label, fn, cls) => {
  const b = el("button", label, cls);
  b.type = "button";
  b.addEventListener("click", () => {
    if (!busy) fn();
  });
  return b;
};
const box = (title) => {
  const e = el("div", undefined, "qo-box");
  if (title) e.append(el("h3", title));
  return e;
};
const sub = (s) => el("p", s, "qo-sub");
const actions = (...items) => {
  const e = el("div", undefined, "qo-actions");
  e.append(...items);
  return e;
};
const pair = (title, value) => {
  const e = el("div", undefined, "qo-pair");
  e.append(el("dt", title), el("dd", value));
  return e;
};
function chips(items) {
  const e = el("div", undefined, "qo-chips");
  items.forEach((s) => e.append(el("span", s, "qo-chip")));
  return e;
}
function titleRow(title, caption, ...buttons) {
  const e = el("div", undefined, "qo-title"),
    left = el("div");
  left.append(el("h2", title));
  if (caption) left.append(el("div", caption, "qo-sub"));
  e.append(left, ...buttons);
  return e;
}
function selectField(parent, id, label, items, value = "", empty) {
  const row = el("div", undefined, "qo-field"),
    caption = el("label", label),
    input = el("select");
  caption.htmlFor = id;
  input.id = id;
  if (empty !== undefined) input.add(new Option(empty, ""));
  items.forEach((i) => {
    const o = new Option(i.label, i.id);
    o.disabled = !!i.disabled;
    input.add(o);
  });
  input.value = value;
  row.append(caption, input);
  parent.append(row);
  return input;
}
function searchableSelectField(parent, id, label, items, value = "", empty) {
  const search = inputField(parent, `${id}-search`, `查找${label}`, "", "search");
  const select = selectField(parent, id, label, [], value, empty);
  let selectedValue = value;
  select.addEventListener("change", () => (selectedValue = select.value));
  const renderOptions = () => {
    const query = search.value.trim().toLocaleLowerCase();
    const matches = items.filter((item) =>
      item.label.toLocaleLowerCase().includes(query)
    );
    const shown = matches.some((item) => item.id === selectedValue)
      ? matches
      : [...items.filter((item) => item.id === selectedValue), ...matches];
    select.replaceChildren();
    if (empty !== undefined) select.add(new Option(empty, ""));
    for (const item of shown) {
      const option = new Option(item.label, item.id);
      option.disabled = !!item.disabled;
      select.add(option);
    }
    if (!shown.length) {
      const unavailable = new Option("没有匹配候选", "");
      unavailable.disabled = true;
      select.add(unavailable);
    }
    select.value = selectedValue;
    if (!select.value && empty !== undefined) select.value = "";
  };
  search.addEventListener("input", renderOptions);
  renderOptions();
  return select;
}
// Query the scoped directory; selected references stay independent of the current page.
function workspaceCandidateField(
  parent,
  id,
  label,
  {
    scope,
    kind,
    purpose,
    position,
    value = "",
    chosen = [],
    known = [],
    multiple = false,
    subject_kind,
    subject_ref,
    onAccessLost = () => {},
  }
) {
  const root = el("div"),
    search = inputField(root, `${id}-search`, `查找${label}`, "", "search"),
    field = multiple
      ? el("div", undefined, "scroll-list")
      : selectField(root, id, label, [], "", `请选择${label}`),
    status = sub("");
  search.maxLength = 80;
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  if (multiple) {
    field.id = id;
    field.setAttribute("aria-label", label);
    root.append(field);
  }
  const selectedRefs = new Set(multiple ? chosen : value ? [value] : []),
    records = new Map();
  for (const item of known)
    if (selectedRefs.has(item.id)) records.set(item.id, { ...item, ref: item.id });
  let items = [],
    cursors = [null],
    nextCursor = null,
    appliedSearch = "",
    generation = 0,
    ready = false;
  const labelFor = (ref) => records.get(ref)?.label || "已有选择（保存时重新核对）";
  const previous = button("上一页", () => {
      if (!ready || cursors.length === 1) return;
      cursors.pop();
      load();
    }),
    next = button("下一页", () => {
      if (!ready || !nextCursor) return;
      cursors.push(nextCursor);
      load();
    }),
    lookup = button("查找 / 重新读取", () => {
      appliedSearch = search.value.trim();
      cursors = [null];
      load();
    });
  search.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      lookup.click();
    }
  });
  function render() {
    field.replaceChildren();
    const refs = [...new Set([...selectedRefs, ...items.map((item) => item.ref)])];
    if (!multiple) field.add(new Option(`请选择${label}`, ""));
    for (const ref of refs) {
      const item = records.get(ref),
        caption = labelFor(ref),
        description = [item?.description, item?.platform, item?.gateway_label]
          .filter(Boolean)
          .join(" · ");
      if (multiple) {
        const row = el("label", undefined, "qo-check"),
          input = el("input"),
          text = el("span", caption);
        input.type = "checkbox";
        input.value = ref;
        input.checked = selectedRefs.has(ref);
        input.disabled = !ready;
        if (description) text.append(sub(description));
        input.addEventListener("change", () => {
          if (input.checked) selectedRefs.add(ref);
          else selectedRefs.delete(ref);
          discardPreview();
        });
        row.append(input, text);
        field.append(row);
      } else
        field.add(new Option(caption + (description ? ` · ${description}` : ""), ref));
    }
    if (!multiple) {
      field.value = [...selectedRefs][0] || "";
      field.disabled = !ready;
    }
    field.inert = !ready;
    field.setAttribute("aria-busy", String(!ready));
    previous.disabled = !ready || cursors.length === 1;
    next.disabled = !ready || !nextCursor;
  }
  if (!multiple)
    field.addEventListener("change", () => {
      selectedRefs.clear();
      if (field.value) selectedRefs.add(field.value);
      discardPreview();
    });
  async function load() {
    const request = ++generation;
    ready = false;
    items = [];
    nextCursor = null;
    discardPreview();
    render();
    status.textContent = "正在读取当前可用候选……";
    const query = new URLSearchParams({
      scope,
      kind,
      purpose,
      search: appliedSearch,
      limit: "50",
    });
    if (purpose === "contact" && position) query.set("position", position);
    if (kind === "channels") {
      query.set("subject_kind", subject_kind);
      query.set("subject_ref", subject_ref);
    }
    if (cursors.at(-1)) query.set("after", cursors.at(-1));
    try {
      const result = await api(`/api/workspace/candidates?${query}`);
      if (!root.isConnected || request !== generation) return;
      if (
        result.scope !== scope ||
        result.kind !== kind ||
        result.purpose !== purpose ||
        (purpose === "contact" &&
          position &&
          !Number.isInteger(result.configuration_version)) ||
        !Array.isArray(result.items) ||
        result.items.length > 50 ||
        result.items.some(
          (item) =>
            typeof item.ref !== "string" ||
            !item.ref ||
            typeof item.label !== "string" ||
            (kind === "channels" &&
              (item.subject_kind !== subject_kind || item.subject_ref !== subject_ref))
        ) ||
        (result.next_cursor !== null && typeof result.next_cursor !== "string")
      )
        throw new Error("候选响应不完整，请重新读取或核对服务版本。");
      items = result.items;
      for (const item of items) records.set(item.ref, item);
      nextCursor = result.next_cursor;
      ready = true;
      render();
      status.textContent = items.length
        ? `第 ${cursors.length} 页 · 本页 ${items.length} 项。已选值跨页保留，保存时重新核验。`
        : `没有找到符合条件的${label}。已有选择保留，保存时重新核验。`;
    } catch (error) {
      if (!root.isConnected || request !== generation) return;
      if (
        purpose === "contact" &&
        position &&
        query.has("after") &&
        error.status === 400 &&
        error.code === "invalid_candidate_query"
      ) {
        cursors = [null];
        return load();
      }
      if (workspaceAccessLost(error)) {
        cursors = [null];
        selectedRefs.clear();
        records.clear();
        onAccessLost();
      }
      render();
      status.textContent = error.message;
    }
  }
  field.candidatesReady = () => ready;
  field.candidateLabel = labelFor;
  field.candidateRecord = (ref) => records.get(ref);
  field.addSelection = (item) => {
    if (!multiple || !item?.ref) return;
    selectedRefs.add(item.ref);
    records.set(item.ref, item);
    render();
    discardPreview();
  };
  root.append(actions(lookup, previous, next), status);
  parent.append(root);
  load();
  return field;
}
function contactSelection(contact) {
  return {
    subject_kind: contact.subject_kind,
    subject_id: contact.subject_id,
    channel_source_link_id: contact.channel_source_link_id,
  };
}
function contactCaption(contact) {
  const kind = contact.subject_kind === "work_account" ? "工作账号" : "个人";
  return `${kind} · ${contact.subject_label || "已保存对象"} · ${contact.channel_label || "已保存渠道"}${contact.platform ? ` · ${contact.platform}` : ""}`;
}
function audienceForCommand(audience) {
  const { authority_grant, contact_basis, ...command } = audience;
  return { ...command, contacts: (audience.contacts || []).map(contactSelection) };
}
function workspaceContactsField(parent, id, scope, audience, peopleField, position) {
  const root = box("已验证联系渠道"),
    selections = new Map(),
    basis = new Map(
      (audience.contact_basis || []).map((item) => [item.channel_source_link_id, item])
    ),
    chosen = el("div", undefined, "scroll-list"),
    message = sub(""),
    subjectHost = el("div"),
    channelHost = el("div");
  message.setAttribute("role", "status");
  message.setAttribute("aria-live", "polite");
  for (const contact of audience.contacts || [])
    selections.set(contact.channel_source_link_id, {
      ...basis.get(contact.channel_source_link_id),
      ...contactSelection(contact),
    });
  root.append(
    sub(
      "先选择人员或工作账号，再选择已验证渠道。加入个人渠道会把该人员列入具体个人名单；工作账号单独保存。最多 20 项，保存前会重新核验。"
    ),
    chosen
  );
  function renderSelections() {
    chosen.replaceChildren();
    if (!selections.size) chosen.append(sub("尚未选择联系渠道。"));
    for (const [ref, contact] of selections) {
      const row = el("div", undefined, "qo-scope-row");
      row.append(
        el("span", contactCaption(contact)),
        button("移除此渠道", () => {
          selections.delete(ref);
          renderSelections();
          discardPreview();
        })
      );
      chosen.append(row);
    }
  }
  function clearSelections() {
    selections.clear();
    renderSelections();
    discardPreview();
  }
  root.append(
    button("清空联系渠道", clearSelections),
    sub(
      "移除或清空渠道保留原个人名单；停止与某个人联系时，还需调整上方的具体个人范围。"
    )
  );
  const subjectKind = selectField(
    root,
    `${id}-kind`,
    "联系对象类型",
    [
      { id: "person", label: "个人" },
      { id: "work_account", label: "工作账号（不对应自然人）" },
    ],
    "person"
  );
  let subjectField, channelField;
  const drawChannels = () => {
    channelHost.replaceChildren();
    channelField = null;
    discardPreview();
    if (!subjectField.value) return;
    channelField = workspaceCandidateField(channelHost, `${id}-channel`, "已验证渠道", {
      scope,
      kind: "channels",
      purpose: "contact",
      position,
      subject_kind: subjectKind.value,
      subject_ref: subjectField.value,
      onAccessLost: clearSelections,
    });
  };
  const drawSubject = () => {
    subjectHost.replaceChildren();
    channelHost.replaceChildren();
    channelField = null;
    message.textContent = "";
    subjectField = workspaceCandidateField(
      subjectHost,
      `${id}-subject`,
      subjectKind.value === "work_account" ? "工作账号" : "联系人员",
      {
        scope,
        kind: subjectKind.value === "work_account" ? "accounts" : "people",
        purpose: "contact",
        position,
        onAccessLost: clearSelections,
      }
    );
    subjectField.addEventListener("change", drawChannels);
  };
  subjectKind.addEventListener("change", drawSubject);
  const ready = () =>
    subjectField.candidatesReady() && (!channelField || channelField.candidatesReady());
  root.append(
    subjectHost,
    channelHost,
    button("加入联系渠道", () => {
      if (!ready() || !subjectField.value || !channelField?.value) {
        message.textContent = "请先读取并选择联系对象和已验证渠道。";
        return;
      }
      const subject = subjectField.candidateRecord(subjectField.value),
        channel = channelField.candidateRecord(channelField.value);
      if (
        !subject ||
        !channel ||
        channel.subject_kind !== subjectKind.value ||
        channel.subject_ref !== subject.ref
      ) {
        message.textContent = "渠道与当前联系对象不一致，请重新读取。";
        return;
      }
      if (selections.has(channel.ref)) {
        message.textContent = "此渠道已经加入，无需重复选择。";
        return;
      }
      if (selections.size >= 20) {
        message.textContent = "最多选择 20 项联系渠道，请先移除不再使用的渠道。";
        return;
      }
      selections.set(channel.ref, {
        subject_kind: subjectKind.value,
        subject_id: subject.ref,
        channel_source_link_id: channel.ref,
        subject_label: subject.label,
        channel_label: channel.label,
        platform: channel.platform,
      });
      if (subjectKind.value === "person") peopleField.addSelection(subject);
      renderSelections();
      discardPreview();
      message.textContent = "已加入本次草稿，请预览并保存后生效。";
    }),
    message
  );
  parent.append(root);
  renderSelections();
  drawSubject();
  return {
    ready,
    value: () => [...selections.values()].map(contactSelection),
    summary: () => [...selections.values()].map(contactCaption).join("；") || "无",
  };
}
function inputField(parent, id, label, value = "", type = "text", required = false) {
  const row = el("div", undefined, "qo-field"),
    caption = el("label", label),
    input = el(type === "textarea" ? "textarea" : "input");
  caption.htmlFor = id;
  input.id = id;
  if (type !== "textarea") input.type = type;
  else input.rows = 3;
  input.value = value;
  input.required = required;
  if (["text", "textarea"].includes(type))
    input.maxLength = type === "text" ? 80 : 2000;
  row.append(caption, input);
  parent.append(row);
  return input;
}
function checkList(parent, id, items, chosen = []) {
  const group = el("div", undefined, "scroll-list");
  group.id = id;
  for (const item of items) {
    const row = el("label", undefined, "qo-check"),
      input = el("input");
    input.type = "checkbox";
    input.value = item.id;
    input.checked = chosen.includes(item.id);
    input.disabled = !!item.disabled;
    row.append(input, el("span", item.label));
    group.append(row);
  }
  parent.append(group);
  return group;
}
function searchableCheckList(parent, id, label, items, chosen = []) {
  const search = inputField(parent, `${id}-search`, `查找${label}`, "", "search");
  const list = checkList(parent, id, items, chosen);
  const none = sub("没有匹配候选");
  none.hidden = true;
  parent.append(none);
  search.addEventListener("input", () => {
    const query = search.value.trim().toLocaleLowerCase();
    let visible = 0;
    for (const row of list.querySelectorAll("label")) {
      row.hidden =
        !row.textContent.toLocaleLowerCase().includes(query) &&
        !row.querySelector("input").checked;
      if (!row.hidden) visible++;
    }
    none.hidden = visible > 0;
  });
  return list;
}
const selected = (id) =>
  [...$(id).querySelectorAll("input:checked")].map((x) => x.value);
const labelOf = (kind, id) =>
  kind === "agents"
    ? agentName(id)
    : state[kind]?.find((x) => x.id === id)?.label || "未关联";
const ledgerFor = (kind, id) =>
  state.organization.ledger.find((x) => x.kind === kind && x.object_ref === id);
const agentName = (id) => ledgerFor("agent", id)?.label || text(id);
const personName = (id) => {
  const p = state.people.find((x) => x.id === id),
    l = ledgerFor("person", id);
  return l
    ? l.label + (l.nickname ? `（${l.nickname}）` : "")
    : p?.label || "待核验人员";
};
function personOptions() {
  return state.people.map((p, i) => {
    const label = personName(p.id),
      same = state.people.filter((x) => personName(x.id) === label).length > 1,
      l = ledgerFor("person", p.id);
    return {
      id: p.id,
      label:
        label +
        (same ? ` · ${l?.description || p.display_name} · 候选 ${i + 1}` : "") +
        (!active(l || {}) ? " · 已停用" : ""),
      disabled: !active(l || {}),
    };
  });
}
function currentRelations(pos) {
  return state.relations.filter(
    (r) =>
      r.role === pos.role_id &&
      r.scope === pos.scope_id &&
      ["active", "scheduled"].includes(r.status)
  );
}
function audienceOf(r) {
  return state.organization.audiences.find((a) => a.collaboration === r.id)
    ?.configuration;
}
function audienceSummary(a) {
  return a
    ? `${{ none: "未选择动态人员", current: "本范围在住人员", past: "本范围过往人员", all: "本范围在住及过往人员" }[a.residents]}；回复：${modeNames[a.reply]}；主动联系：${modeNames[a.proactive]}`
    : "尚未配置触达对象";
}
function details(title, ...content) {
  const d = el("details");
  d.append(el("summary", title), ...content);
  return d;
}
function empty(parent, message, next) {
  parent.append(sub(message));
  if (next) parent.append(next);
}
