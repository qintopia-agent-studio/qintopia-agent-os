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
  const row = el("label", label, "qo-field"),
    input = el("select");
  input.id = id;
  if (empty !== undefined) input.add(new Option(empty, ""));
  items.forEach((i) => {
    const o = new Option(i.label, i.id);
    o.disabled = !!i.disabled;
    input.add(o);
  });
  input.value = value;
  row.append(input);
  parent.append(row);
  return input;
}
function inputField(parent, id, label, value = "", type = "text", required = false) {
  const row = el("label", label, "qo-field"),
    input = el(type === "textarea" ? "textarea" : "input");
  input.id = id;
  if (type !== "textarea") input.type = type;
  else input.rows = 3;
  input.value = value;
  input.required = required;
  if (["text", "textarea"].includes(type))
    input.maxLength = type === "text" ? 80 : 2000;
  row.append(input);
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
    (r) => r.role === pos.role_id && r.scope === pos.scope_id && r.status === "active"
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
