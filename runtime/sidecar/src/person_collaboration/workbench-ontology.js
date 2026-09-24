/* Read-only ontology views and governed identity actions. Service facts stay authoritative. */
const ontologyRequests = new Map();
let helpSequence = 0;

function displayTime(value, absent = "未记录") {
  if (!value) return absent;
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? "时间待核对" : date.toLocaleString("zh-CN");
}
function localDateTime(value) {
  const date = value ? new Date(value) : null;
  return date && !Number.isNaN(date.getTime())
    ? new Date(date.getTime() - date.getTimezoneOffset() * 60000)
        .toISOString()
        .slice(0, 16)
    : "";
}
function termSummary(relation) {
  return `${displayTime(relation.valid_from, "开始时间未记录")}起，${
    relation.valid_until
      ? `至 ${displayTime(relation.valid_until)}`
      : "持续至离任或撤销"
  }`;
}
function setHelpOpen(wrapper, open) {
  if (open)
    document.querySelectorAll(".qo-help").forEach((other) => {
      if (other !== wrapper) setHelpOpen(other, false);
    });
  const trigger = wrapper.querySelector("button");
  const popup = wrapper.querySelector('[role="tooltip"]');
  trigger.setAttribute("aria-expanded", String(open));
  popup.hidden = !open;
  if (!open) {
    wrapper.dataset.pinned = "false";
    return;
  }
  const anchor = trigger.getBoundingClientRect();
  const margin = 12;
  const viewportWidth = Math.min(
    window.innerWidth,
    document.documentElement.clientWidth
  );
  const viewportHeight = Math.min(
    window.innerHeight,
    document.documentElement.clientHeight
  );
  const width = Math.min(300, viewportWidth - margin * 2);
  popup.style.width = `${width}px`;
  popup.style.maxWidth = `${viewportWidth - margin * 2}px`;
  popup.style.maxHeight = `${viewportHeight - margin * 2}px`;
  popup.style.left = `${Math.max(margin, Math.min(anchor.left, viewportWidth - width - margin))}px`;
  popup.style.top = "0px";
  const height = popup.getBoundingClientRect().height;
  popup.style.top = `${Math.max(margin, Math.min(anchor.bottom + 8, viewportHeight - height - margin))}px`;
}
function helpTip(label, explanation) {
  const wrapper = el("span", undefined, "qo-help");
  const trigger = el("button", "?", "qo-help-trigger");
  const popup = el("span", explanation, "qo-tooltip");
  let hideTimer;
  popup.id = `help-${++helpSequence}`;
  popup.setAttribute("role", "tooltip");
  popup.hidden = true;
  trigger.type = "button";
  trigger.setAttribute("aria-label", `${label}说明`);
  trigger.setAttribute("aria-describedby", popup.id);
  trigger.setAttribute("aria-expanded", "false");
  wrapper.append(trigger, popup);
  wrapper.addEventListener("pointerenter", (event) => {
    clearTimeout(hideTimer);
    if (event.pointerType === "mouse") setHelpOpen(wrapper, true);
  });
  wrapper.addEventListener("pointerleave", () => {
    if (wrapper.dataset.pinned !== "true" && document.activeElement !== trigger)
      hideTimer = setTimeout(() => setHelpOpen(wrapper, false), 180);
  });
  trigger.addEventListener("focus", () => setHelpOpen(wrapper, true));
  trigger.addEventListener("blur", () => setHelpOpen(wrapper, false));
  trigger.addEventListener("click", (event) => {
    event.preventDefault();
    event.stopPropagation();
    const pin = wrapper.dataset.pinned !== "true";
    setHelpOpen(wrapper, pin);
    wrapper.dataset.pinned = String(pin);
  });
  return wrapper;
}
function explainHeading(panel, label, explanation) {
  const heading = panel.querySelector("h3, h4, legend");
  if (heading) heading.append(helpTip(label, explanation));
}
function fieldExplanation(input, label, explanation) {
  const row = input.parentElement;
  const line = el("span", undefined, "qo-field-label");
  const caption = row.querySelector("label");
  if (caption) line.append(caption);
  line.append(helpTip(label, explanation));
  row.prepend(line);
}
document.addEventListener("keydown", (event) => {
  if (event.key !== "Escape") return;
  document
    .querySelectorAll(".qo-help")
    .forEach((wrapper) => setHelpOpen(wrapper, false));
});
document.addEventListener("click", (event) => {
  document.querySelectorAll(".qo-help").forEach((wrapper) => {
    if (!wrapper.contains(event.target)) setHelpOpen(wrapper, false);
  });
});
window.addEventListener("resize", () => {
  document
    .querySelectorAll(".qo-help")
    .forEach((wrapper) => setHelpOpen(wrapper, false));
});
document.addEventListener(
  "scroll",
  (event) => {
    if (event.target.closest?.(".qo-tooltip")) return;
    document
      .querySelectorAll(".qo-help")
      .forEach((wrapper) => setHelpOpen(wrapper, false));
  },
  true
);

function ontologyFor(scope) {
  const key = `${state.version}:${scope}`;
  if (!ontologyRequests.has(key)) {
    // Coalesce only concurrent reads. A version can expire without a new save.
    const request = api(`/api/ontology?scope=${encodeURIComponent(scope)}`).finally(
      () => ontologyRequests.delete(key)
    );
    ontologyRequests.set(key, request);
  }
  return ontologyRequests.get(key);
}
function sourceDetails(record) {
  const source = record.source || {};
  const dl = el("dl");
  dl.append(
    pair("来源", source.label || "来源名称未记录"),
    pair("当前版本", record.version == null ? "未记录" : `第 ${record.version} 版`),
    pair("开始适用", displayTime(record.effective_at)),
    pair("截至", displayTime(record.effective_until, "未设截止时间"))
  );
  if (record.author) dl.append(pair("记录人", record.author.label || "姓名未记录"));
  const technical = [];
  if (source.kind) technical.push(`来源类型：${source.kind}`);
  if (source.ref) technical.push(`来源引用：${source.ref}`);
  if (record.id) technical.push(`版本引用：${record.id}`);
  if (technical.length) dl.append(details("技术出处", el("pre", technical.join("\n"))));
  return details("查看来源与生效时间", dl);
}
function readableContent(value) {
  if (typeof value === "string") return value;
  if (!value || typeof value !== "object") return "内容未记录，请核对来源。";
  for (const key of ["text", "body", "statement", "summary", "description"])
    if (typeof value[key] === "string") return value[key];
  return null;
}
function constraintsPanel(scope) {
  const panel = box("这项工作受哪些共同约束");
  explainHeading(
    panel,
    "共同约束",
    "这里引用当前范围适用的已确认规则及其版本。上层约束仍然有效；任职或楼栋设置不能把它放宽。此处只读，不维护具体业务习惯。"
  );
  const content = el("div");
  content.setAttribute("aria-live", "polite");
  content.append(sub("正在核对本范围适用的依据……"));
  panel.append(content);
  ontologyFor(scope)
    .then((data) => {
      if (!panel.isConnected) return;
      content.replaceChildren();
      if (!Array.isArray(data.constraints))
        throw new Error("约束响应不完整，请重新读取后核对。");
      if (!data.constraints.length) {
        content.append(
          sub(
            "当前没有可展示的已确认共同约束。请由有权负责人补齐依据；这不表示可以自行放宽其他授权或业务门禁。"
          )
        );
        return;
      }
      for (const rule of data.constraints) {
        const row = el("article", undefined, "qo-scope-row");
        row.append(
          el("h4", rule.title || "已确认规则"),
          chips([
            rule.inherited ? "继承上层约束" : "本范围依据",
            rule.scope?.label || "适用范围未记录",
            rule.version == null ? "版本未记录" : `第 ${rule.version} 版`,
          ])
        );
        const description = readableContent(rule.content);
        if (description) row.append(el("p", description));
        else
          row.append(
            details(
              "查看完整规则内容",
              el("pre", JSON.stringify(rule.content, null, 2))
            )
          );
        row.append(sourceDetails(rule));
        content.append(row);
      }
    })
    .catch((error) => {
      if (panel.isConnected) content.replaceChildren(sub(error.message));
    });
  return panel;
}

const capabilityStatusNames = {
  local_ready: "本地可验收",
  available: "本地可用",
  connected: "本地已接入",
  local: "本地已接入",
  ready: "本地就绪",
  disabled: "尚未启用",
  unavailable: "暂不可执行",
  pending: "待接入",
  registered: "已登记，待接入",
  not_connected: "尚未接入",
};
function readinessPanel(agent, scope) {
  const panel = box("现在能否执行");
  explainHeading(
    panel,
    "能力与配置状态",
    "登记智能体、保存合作配置、本地消费者就绪和真实渠道启用是不同状态。只有实际消费者及开关的结果能说明目前可执行什么。"
  );
  const content = el("div");
  content.append(sub("正在读取能力与执行器状态……"));
  panel.append(content);
  if (!scope) {
    content.replaceChildren(sub("当前没有可查看的工作范围，请先选择获授权的岗位。"));
    return panel;
  }
  ontologyFor(scope)
    .then((data) => {
      if (!panel.isConnected) return;
      content.replaceChildren();
      const capability = data.capabilities?.find((item) => item.agent_key === agent);
      if (!capability) {
        content.append(
          sub("服务端尚未提供此智能体的能力状态，请联系维护负责人核对接入情况。")
        );
        return;
      }
      const dl = el("dl");
      dl.append(
        pair("查看范围", data.scope?.label || "范围待核对"),
        pair("登记", statusNames[capability.registration_status] || "登记状态待核对"),
        pair(
          "本范围配置",
          capability.configured ? "已保存工作安排" : "尚未保存工作安排"
        ),
        pair(
          "本地执行",
          capabilityStatusNames[capability.local_status] || "执行状态待核对"
        ),
        pair(
          "真实渠道",
          capability.real_channel_enabled === true
            ? "服务端报告已启用，具体事项仍需核验权限"
            : capability.real_channel_enabled === false
              ? "尚未启用"
              : "状态未提供，不能确认可用"
        )
      );
      content.append(dl);
      for (const item of capability.capabilities || []) {
        const row = el("div", undefined, "qo-scope-row");
        row.append(
          el("strong", item.label || "已登记能力"),
          sub(capabilityStatusNames[item.status] || "状态待核对")
        );
        if (item.reason) row.append(sub(item.reason));
        if (item.key) row.append(details("技术出处", el("code", item.key)));
        content.append(row);
      }
      content.append(
        sub("配置保存不会自动启用外部渠道；每次执行仍使用当时有效的身份、任期与授权。")
      );
    })
    .catch((error) => {
      if (panel.isConnected) content.replaceChildren(sub(error.message));
    });
  return panel;
}

const audienceReasons = {
  pms_source_not_bound: "本范围尚未关联可信住宿来源",
  pms_source_unavailable: "住宿来源目前不可用，请由来源维护者核对",
  pms_source_rebuilding: "住宿来源正在重建，暂不判断当前状态",
  pms_projection_invalidated: "住宿依据已失效，等待来源核对",
  pms_projection_conflicted: "住宿来源存在冲突，不能据此确定当前状态",
  pms_projection_unconfirmed: "住宿来源尚未确认",
  pms_projection_stale: "住宿观测已过期或时间异常，等待来源重新观测",
  pms_arrangement_unconfirmed: "当前住宿安排尚未确认",
  shared_account_person_unknown: "共享账号无法确定具体自然人",
  person_outside_tenant: "来源人员不属于当前可核验范围",
  source_stale: "住宿资料已过期，等待来源重新观测",
  stale_source: "住宿资料已过期，等待来源重新观测",
  pms_stale: "住宿资料已过期，等待来源重新观测",
  identity_unconfirmed: "身份关联尚未确认",
  shared_account: "共享账号不能直接认作某个人",
  source_conflicted: "住宿来源存在冲突",
  source_rebuilding: "来源正在重建",
  source_invalidated: "来源已失效",
  explicit_selection: "由这项工作明确选择",
  current_stay: "本范围有可靠的当前入住依据",
  past_stay: "本范围有可靠的过往入住依据",
};
function audienceReason(reason) {
  return audienceReasons[reason] || "相关来源尚需核对";
}
function audiencePreview(relation, draft = false) {
  const panel = details("查看实际会覆盖哪些人");
  panel.append(
    sub(
      draft
        ? "以下按已保存的联系范围核对。本次尚未保存的修改不会提前生效。"
        : "只读取此项工作可见的对象和住宿依据，不发送消息。名单在执行时还会重新核对。"
    )
  );
  const output = el("div", undefined, "qo-audience-preview");
  output.setAttribute("aria-live", "polite");
  const inspect = button("核对当前对象", async () => {
    inspect.disabled = true;
    output.replaceChildren(sub("正在核对人员与当前来源……"));
    try {
      const result = await api("/api/audience-preview", { collaboration: relation.id });
      if (!panel.isConnected) return;
      output.replaceChildren();
      const completeness = {
        complete: "本次依据完整",
        partial: "部分对象仍待核对",
        unavailable: "暂时无法形成可靠名单",
      };
      output.append(
        el(
          "p",
          `${result.scope_label || labelOf("scopes", relation.scope)} · ${completeness[result.completeness] || "完整性待核对"}`
        ),
        sub(
          `本次核对：${displayTime(result.observed_at)}。实际纳入 ${result.counts?.selected ?? "未提供"} 人。`
        )
      );
      for (const [status, label] of [
        ["current", "目前在住"],
        ["past", "曾经住过"],
        ["explicit", "明确选择的联系对象"],
        ["unknown", "当前状态待核对"],
      ]) {
        const people = (result.people || []).filter(
          (person) => person.status === status
        );
        if (!people.length) continue;
        const section = el("div", undefined, "qo-scope-row");
        section.append(el("h4", `${label} · ${people.length} 人`));
        for (const person of people) {
          const row = el("div", undefined, "qo-audience-person");
          row.append(
            el("strong", person.label || "姓名未提供"),
            el("span", person.selected ? "纳入本项联系范围" : "本次不纳入", "qo-sub")
          );
          if (person.reasons?.length)
            row.append(sub(person.reasons.map(audienceReason).join("；")));
          section.append(row);
        }
        output.append(section);
      }
      if (!(result.people || []).length)
        output.append(sub("当前没有可展示的已关联人员。这不等于该范围没有在住人员。"));
      for (const unresolved of result.unresolved || [])
        output.append(
          sub(
            `${audienceReason(unresolved.reason)}：${unresolved.count} 条待核对来源。`
          )
        );
      if (result.completeness !== "complete")
        output.append(
          el(
            "p",
            "资料不确定时，不把人判断为不在住，也不会把未知对象自动纳入动态联系。",
            "qo-warning"
          )
        );
    } catch (error) {
      if (panel.isConnected) output.replaceChildren(sub(error.message));
    } finally {
      inspect.disabled = false;
    }
  });
  panel.append(inspect, output);
  return panel;
}

function identitySourceLabel(link) {
  const label = link.source_label || "来源未记录";
  const scope = link.scope_label;
  return scope && !label.includes(scope) ? `${label} · ${scope}` : label;
}
function identityPanel(item) {
  const panel = box("账号与本人如何关联");
  explainHeading(
    panel,
    "身份核验",
    "只核对已登记来源中的账号是否属于此人。同名不会自动合并，确认身份不会自动授予岗位或业务权限。共享账号不能直接归为某个自然人。"
  );
  const content = el("div");
  content.setAttribute("aria-live", "polite");
  content.append(sub("正在读取此人的来源身份……"));
  panel.append(content);
  const person = item.object_ref;
  if (!person) {
    content.replaceChildren(sub("尚未建立可核验的人员档案，请先保存人员登记。"));
    return panel;
  }
  api(`/api/identities?person=${encodeURIComponent(person)}`)
    .then((data) => {
      if (!panel.isConnected) return;
      content.replaceChildren();
      if (!data.can_manage) {
        content.append(
          sub(
            "当前账号没有来源身份管理权。请由获授权的身份管理者核对，不通过登记或任职间接取得身份权限。"
          )
        );
        return;
      }
      const links = data.links || [];
      if (!links.length)
        content.append(
          sub("尚无已关联账号。请先核对下方候选的来源和账号，再确认是否属于此人。")
        );
      for (const link of links) {
        const row = el("div", undefined, "qo-scope-row");
        row.append(
          el("h4", link.account_label || "账号名称未记录"),
          sub(
            `${identitySourceLabel(link)} · ${statusNames[link.status] || "状态待核对"}`
          ),
          pair("对应人员", link.person_label || item.label)
        );
        if (link.status === "confirmed") {
          const revoke = button(
            "核对撤销影响",
            () => identityPreview(data, link, person, true, panel),
            "qo-danger"
          );
          revoke.disabled = !link.selectable;
          row.append(actions(revoke));
          if (!link.selectable)
            row.append(
              sub(link.unavailable_reason || "当前来源不能完成身份变更，请先核对登记。")
            );
        }
        const provenance = el("dl");
        provenance.append(
          pair("关联版本", link.version == null ? "未记录" : String(link.version)),
          pair("来源类型", link.subject_type || "未记录")
        );
        if (link.gateway_key) provenance.append(pair("登记来源", link.gateway_key));
        row.append(details("查看核验出处", provenance));
        content.append(row);
      }
      const candidates = data.candidates || [];
      const available = candidates.filter((candidate) => candidate.selectable);
      const form = el("form", undefined, "qo-identity-form");
      form.append(el("h4", `为${item.label}核验来源账号`));
      const candidate = selectField(
        form,
        "identity-candidate",
        "待核对的账号",
        candidates.map((link) => ({
          id: link.id,
          label: `${link.account_label || "账号名称未记录"} · ${identitySourceLabel(link)}${link.selectable ? "" : " · 暂不可关联"}`,
          disabled: !link.selectable,
        })),
        "",
        "请选择已登记来源中的账号"
      );
      candidate.required = true;
      const account = el("div", undefined, "qo-identity-account");
      account.setAttribute("aria-live", "polite");
      candidate.addEventListener("change", () => {
        account.replaceChildren();
        const link = candidates.find((entry) => entry.id === candidate.value);
        if (!link) return;
        account.append(
          pair("确认属于", item.label),
          pair("来源账号", link.account_label || "名称未记录"),
          pair("来源范围", `${identitySourceLabel(link)}`),
          pair(
            "账号性质",
            link.account_kind === "shared"
              ? "共享账号，不能确认自然人"
              : link.account_kind === "personal"
                ? "个人账号，仍须核对本人"
                : "账号性质待核对"
          )
        );
        if (link.person_label) account.append(pair("当前关联", link.person_label));
      });
      const submit = el("button", "核对关联影响", "qo-primary");
      submit.type = "submit";
      submit.disabled = !available.length;
      form.append(
        account,
        sub("请依据实际来源证据核对，姓名相似本身不能证明是同一人。"),
        actions(submit)
      );
      form.addEventListener("submit", (event) => {
        event.preventDefault();
        const link = available.find((entry) => entry.id === candidate.value);
        if (link) identityPreview(data, link, person, false, panel);
      });
      content.append(form);
      if (!available.length)
        content.append(
          sub("当前没有可以确认的账号候选。请先由来源接入方补齐可信观测，再重新读取。")
        );
      for (const link of candidates.filter((entry) => !entry.selectable))
        content.append(
          sub(
            `${link.account_label || "待核对账号"}：${link.unavailable_reason || "当前不满足身份核验条件"}`
          )
        );
    })
    .catch((error) => {
      if (panel.isConnected) content.replaceChildren(sub(error.message));
    });
  return panel;
}
async function identityPreview(data, link, person, revoke, host) {
  if (busy) return;
  discardPreview();
  setBusy(true);
  notice("正在核对身份变更与受影响工作……");
  try {
    const command = {
      operation_id: crypto.randomUUID(),
      link_ref: link.id,
      person_ref: person,
      expected_version: link.version,
      expected_configuration_version: data.version,
      expected_gateway_version: link.gateway_version,
      revoke,
    };
    const checked = await api("/api/identities/preview", command);
    if (!host.isConnected) return;
    if (
      checked.kind !== "identity_change" ||
      checked.person_ref !== person ||
      !checked.impact ||
      checked.after?.status !== (revoke ? "revoked" : "confirmed")
    )
      throw new Error("服务端未返回对应人员的完整核验预览，请重新读取后核对。");
    const panel = el("div", undefined, "qo-preview");
    panel.id = "preview";
    panel.tabIndex = -1;
    panel.append(el("h3", revoke ? "撤销后会发生什么" : "确认这是同一个人"));
    const dl = el("dl");
    dl.append(
      pair("人员", checked.person_label || personName(person)),
      pair("来源账号", checked.account_label || link.account_label || "名称未记录"),
      pair("来源", checked.source_label || link.source_label || "未记录"),
      pair("变更前", statusNames[checked.before?.status] || "原状态未记录"),
      pair("变更后", statusNames[checked.after?.status] || "新状态待核对")
    );
    panel.append(dl);
    const impact = checked.impact || {};
    panel.append(
      el(
        "p",
        impact.description || "服务端未提供完整影响说明，请先核对来源。",
        "qo-note"
      )
    );
    const counts = el("dl");
    if (impact.revokes_sessions !== undefined)
      counts.append(
        pair(
          "账号会话",
          typeof impact.revokes_sessions === "number"
            ? `使 ${impact.revokes_sessions} 个会话失效`
            : impact.revokes_sessions
              ? "相关旧会话将失效"
              : "服务端未报告需撤销的会话"
        )
      );
    if (impact.ends_appointments !== undefined)
      counts.append(pair("任职", `结束 ${impact.ends_appointments} 项`));
    if (impact.revokes_grants !== undefined)
      counts.append(pair("业务权限", `撤销 ${impact.revokes_grants} 项`));
    if (impact.removes_last_verification)
      counts.append(pair("人员核验", "将失去最后一项有效身份核验"));
    if (impact.activates_person)
      counts.append(pair("人员状态", "身份核验后成为可另行安排任职的人员"));
    panel.append(
      counts,
      sub("历史记录保留；身份确认不会授予业务权限，重新关联也不会恢复旧授权。")
    );
    pending = {
      command,
      savePath: "/api/identities/save",
      successMessage: revoke
        ? "身份关联已撤销，已重新读取当前核验与工作状态。"
        : "身份关联已确认。任职与业务权限仍需单独配置。",
    };
    panel.append(
      actions(
        button(
          revoke ? "确认撤销关联" : "确认关联本人",
          savePending,
          revoke ? "qo-danger" : "qo-primary"
        ),
        button("返回核对", discardPreview)
      )
    );
    host.append(panel);
    notice("尚未保存，请核对人员、来源账号和实际影响。");
  } catch (error) {
    onError(error);
  } finally {
    setBusy(false);
    $("preview")?.focus({ preventScroll: false });
  }
}

const historyFieldNames = {
  label: "名称",
  nickname: "昵称",
  description: "说明",
  person: "任职人员",
  person_id: "任职人员",
  person_ref: "核验人员",
  agent: "协作智能体",
  agent_key: "协作智能体",
  role: "岗位",
  role_id: "岗位",
  scope: "工作范围",
  scope_id: "工作范围",
  duty: "承担职责",
  duty_id: "承担职责",
  responsibility: "负责什么",
  responsibility_text: "负责什么",
  status: "状态",
  valid_from: "任期开始",
  valid_until: "任期截止",
  permissions: "决定权",
  grants: "决定权",
  groups: "联系群",
  people: "具体联系人",
  residents: "动态人员范围",
  reply: "回复方式",
  proactive: "主动联系",
  reviewer: "确认人",
  reviewer_person_id: "确认人",
  open_reception: "公开咨询接待",
  topics: "联系目的",
  visibility: "信息边界",
  connections: "关联工作",
};
function historyValue(key, value) {
  if (value === undefined) return "未记录";
  if (value === null || value === "") return key === "valid_until" ? "未设截止" : "无";
  if (
    ["person", "person_id", "person_ref", "reviewer", "reviewer_person_id"].includes(
      key
    )
  )
    return state.people.some((person) => person.id === value) ||
      ledgerFor("person", value)
      ? personName(value)
      : "姓名未记录或当前不可见";
  if (["agent", "agent_key"].includes(key)) return agentName(value);
  for (const [prefix, collection] of [
    ["role", "roles"],
    ["scope", "scopes"],
    ["duty", "duties"],
  ])
    if ([prefix, `${prefix}_id`].includes(key)) return labelOf(collection, value);
  if (["valid_from", "valid_until"].includes(key)) return displayTime(value);
  if (key === "status") return statusNames[value] || "状态未识别";
  if (["reply", "proactive"].includes(key)) return modeNames[value] || "决定方式未记录";
  if (key === "residents")
    return (
      {
        none: "不选动态人员",
        current: "本范围在住人员",
        past: "本范围过往人员",
        all: "本范围在住与过往人员",
      }[value] || "范围未识别"
    );
  if (key === "visibility")
    return (
      { general: "范围内一般服务信息", service_private: "事项必要的服务信息" }[value] ||
      "边界未识别"
    );
  if (key === "open_reception") return value ? "承担公开咨询接待" : "不承担开放接待";
  if (key === "groups" && Array.isArray(value))
    return value.map((id) => labelOf("groups", id)).join("、") || "无";
  if (key === "people" && Array.isArray(value))
    return value.map(personName).join("、") || "无";
  if (key === "connections" && Array.isArray(value))
    return (
      value
        .map(
          (connection) =>
            `${agentName(connection.agent_key)} · ${labelOf("duties", connection.duty_id)} · ${statusNames[connection.status] || "状态未记录"}`
        )
        .join("；") || "无"
    );
  if (["permissions", "grants"].includes(key) && Array.isArray(value))
    return (
      value
        .map(
          (grant) =>
            `${text(grant.action || grant.action_key)}：${modeNames[grant.mode || grant.decision_mode] || "状态未记录"}${grant.reviewer || grant.reviewer_person_id ? `（${historyValue("reviewer", grant.reviewer || grant.reviewer_person_id)}确认）` : ""}${grant.status && grant.status !== "active" ? "，已失效" : ""}`
        )
        .join("；") || "未授予"
    );
  return typeof value === "string" || typeof value === "number"
    ? String(value)
    : "查看详细记录";
}
function historyCanonical(value, key = "") {
  if (Array.isArray(value)) {
    const values = ["permissions", "grants"].includes(key)
      ? value
          .filter((grant) => !grant.status || grant.status === "active")
          .map((grant) => ({
            action: grant.action || grant.action_key,
            mode: grant.mode || grant.decision_mode,
            reviewer: grant.reviewer || grant.reviewer_person_id || null,
            include_descendants: grant.include_descendants || false,
            managed_agents: grant.managed_agents || [],
            managed_domains: grant.managed_domains || [],
            managed_actions: grant.managed_actions || [],
            delegation_depth: grant.delegation_depth || 0,
          }))
      : value;
    return values
      .map((item) => historyCanonical(item))
      .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  }
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.keys(value)
        .sort()
        .map((field) => [field, historyCanonical(value[field], field)])
    );
  return value;
}
function historySnapshot(value) {
  if (!value || typeof value !== "object") return {};
  const result = {};
  for (const key of Object.keys(historyFieldNames))
    if (key in value) result[key] = historyCanonical(value[key], key);
  for (const container of [
    "appointment",
    "connection",
    "assignment",
    "audience",
    "configuration",
  ])
    Object.assign(result, historySnapshot(value[container]));
  return result;
}
function historyPanel() {
  const panel = details("最近变更：谁改了什么、影响了谁");
  panel.className = "ql-history";
  const records = state.organization.history || [];
  if (!records.length) {
    panel.append(sub("当前没有可展示的变更记录。历史只向有权查看的人员提供。"));
    return panel;
  }
  const kinds = {
    configure_work: "调整任职、决定权与联系范围",
    assign: "调整任职与决定权",
    save_ledger: "维护基础台账",
    save_position: "调整岗位归属",
    lifecycle: "调整在册状态",
    end_appointment: "结束任职",
    end_collaboration: "解除协作",
    set_groups: "调整群范围关联",
    set_audience: "调整联系范围",
    save_role: "调整岗位职责",
    save_duty: "调整职责定义",
    identity_change: "核对来源身份",
  };
  for (const record of records) {
    const change = record.result?.change || record.change || {};
    const before = historySnapshot(record.before ?? change.before);
    const after = historySnapshot(record.after ?? change.after);
    const row = el("article", undefined, "qo-history-entry");
    row.append(
      el("h4", kinds[record.kind || change.kind] || "配置变更"),
      sub(`${record.actor_label || "操作者未记录"} · ${displayTime(record.created_at)}`)
    );
    if ((record.kind || change.kind) === "identity_change") {
      const subject = el("dl");
      const person = change.person_ref || record.person_ref;
      subject.append(
        pair(
          "核验人员",
          change.person_label ||
            record.person_label ||
            (person ? historyValue("person_ref", person) : "人员未记录")
        ),
        pair("身份来源", change.source_label || record.source_label || "来源未记录"),
        pair(
          "来源账号",
          change.account_label || record.account_label || "账号名称未记录"
        )
      );
      row.append(subject);
    }
    const differences = el("div", undefined, "qo-history-changes");
    const keys = [...new Set([...Object.keys(before), ...Object.keys(after)])].filter(
      (key) => JSON.stringify(before[key]) !== JSON.stringify(after[key])
    );
    for (const key of keys) {
      const line = el("div", undefined, "qo-history-diff");
      line.append(
        el("strong", historyFieldNames[key]),
        el("span", `原来：${historyValue(key, before[key])}`),
        el("span", `现在：${historyValue(key, after[key])}`)
      );
      differences.append(line);
    }
    if (!keys.length)
      differences.append(
        sub(
          Object.keys(before).length || Object.keys(after).length
            ? "已记录的设置与变更前一致。"
            : "这条记录未保留可比较的前后快照，不能还原为完整变更。"
        )
      );
    row.append(differences);
    const impact = record.impact ?? change.impact;
    if (typeof impact === "string") row.append(el("p", impact, "qo-note"));
    else if (Array.isArray(impact))
      impact.forEach((message) => row.append(sub(String(message))));
    else if (impact && typeof impact === "object") {
      if (impact.description) row.append(el("p", impact.description, "qo-note"));
      for (const [key, label] of [
        ["ended_connections", "结束协作"],
        ["ends_appointments", "结束任职"],
        ["revokes_grants", "撤销授权"],
        ["revokes_sessions", "撤销旧会话"],
      ])
        if (typeof impact[key] === "number")
          row.append(sub(`${label}：${impact[key]} 项`));
    } else if (change.ended_connections !== undefined)
      row.append(sub(`实际结束 ${change.ended_connections} 项协作。`));
    else row.append(sub("此记录未单独记录影响范围，请结合当时的工作安排核对。"));
    row.append(
      details(
        "查看原始变更出处",
        el(
          "pre",
          JSON.stringify(
            {
              before: record.before ?? change.before ?? null,
              after: record.after ?? change.after ?? null,
              change,
            },
            null,
            2
          )
        )
      )
    );
    panel.append(row);
  }
  return panel;
}
