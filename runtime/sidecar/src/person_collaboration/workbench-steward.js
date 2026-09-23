/* Everyday work uses the same authenticated services as trusted Agent tools. */
function workTabs(label, entries) {
  const root = el("div", undefined, "qo-work-tabs"),
    nav = el("div", undefined, "qo-tabs"),
    id = crypto.randomUUID();
  nav.setAttribute("role", "tablist");
  nav.setAttribute("aria-label", label);
  const buttons = [],
    panels = [];
  function choose(n, focus = false) {
    buttons.forEach((b, i) => {
      b.setAttribute("aria-selected", String(i === n));
      b.tabIndex = i === n ? 0 : -1;
      panels[i].hidden = i !== n;
    });
    if (focus) buttons[n].focus();
    panels[n].refreshWorkspace?.();
  }
  entries.forEach(([name, node], i) => {
    const b = button(name, () => choose(i));
    b.id = `${id}-tab-${i}`;
    b.setAttribute("role", "tab");
    node.id = `${id}-panel-${i}`;
    node.setAttribute("role", "tabpanel");
    node.setAttribute("aria-labelledby", b.id);
    b.setAttribute("aria-controls", node.id);
    node.tabIndex = 0;
    b.addEventListener("keydown", (e) => {
      const n =
        e.key === "ArrowRight"
          ? (i + 1) % entries.length
          : e.key === "ArrowLeft"
            ? (i + entries.length - 1) % entries.length
            : e.key === "Home"
              ? 0
              : e.key === "End"
                ? entries.length - 1
                : -1;
      if (n >= 0) {
        e.preventDefault();
        choose(n, true);
      }
    });
    nav.append(b);
    buttons.push(b);
    panels.push(node);
  });
  root.append(nav, ...panels);
  choose(0);
  return root;
}

function knowledgeHistory(item, dates) {
  const root = box("历史记录"),
    list = el("div"),
    detail = el("div"),
    controls = actions();
  const query = inputField(root, crypto.randomUUID(), "搜索历史内容或修改人");
  const filter = selectField(
    root,
    crypto.randomUUID(),
    "记录状态",
    [
      { id: "all", label: "全部记录" },
      { id: "active", label: "未撤回" },
      { id: "withdrawn", label: "已撤回或取消" },
    ],
    "all"
  );
  const since = inputField(root, crypto.randomUUID(), "生效日期从", "", "date"),
    until = inputField(root, crypto.randomUUID(), "生效日期至", "", "date");
  let page = 0;
  function draw() {
    const records = item.revisions.filter(
      (r) =>
        `${r.author || ""} ${r.content.title || ""} ${r.content.text || ""}`
          .toLowerCase()
          .includes(query.value.trim().toLowerCase()) &&
        (filter.value === "all" ||
          Boolean(r.withdrawn_at) === (filter.value === "withdrawn")) &&
        (!since.value ||
          new Date(r.effective_at) >= new Date(since.value + "T00:00:00")) &&
        (!until.value ||
          new Date(r.effective_at) <= new Date(until.value + "T23:59:59"))
    );
    page = Math.min(page, Math.max(0, Math.ceil(records.length / 10) - 1));
    list.replaceChildren();
    detail.replaceChildren();
    controls.replaceChildren();
    records.slice(page * 10, page * 10 + 10).forEach((r) => {
      const row = el("article", undefined, "qo-scope-row");
      row.append(
        el("strong", `第 ${r.version} 版 · ${r.content.title || "知识内容"}`),
        sub(
          `${r.author || "已核验人员"} · ${dates(r)}${r.withdrawn_at ? " · 已撤回" : ""}`
        ),
        button("查看内容与上一版", () => {
          detail.replaceChildren(
            el("h4", `第 ${r.version} 版`),
            el("pre", r.content.text, "qo-readable-text")
          );
          const prior = item.revisions.find((p) => p.version < r.version);
          if (prior)
            detail.append(
              el("h4", `上一版（第 ${prior.version} 版）`),
              el("pre", prior.content.text, "qo-readable-text")
            );
          detail.focus();
        })
      );
      list.append(row);
    });
    const prev = button("上一页", () => {
        page--;
        draw();
      }),
      next = button("下一页", () => {
        page++;
        draw();
      });
    prev.disabled = page === 0;
    next.disabled = (page + 1) * 10 >= records.length;
    controls.append(prev, sub(`第 ${page + 1} 页 · 共 ${records.length} 条`), next);
  }
  detail.tabIndex = -1;
  [query, filter, since, until].forEach((field) =>
    field.addEventListener("input", () => {
      page = 0;
      draw();
    })
  );
  root.append(list, controls, detail);
  draw();
  return root;
}

function stewardWorkspace(scope) {
  const chat = box("与二花一起工作"),
    settings = el("div"),
    progress = el("div");
  chat.append(
    sub(
      "把事情交代给二花，或在其他标签里直接维护。当前为本地固定例句验证，真实模型与微信对话尚未接通。"
    )
  );
  const form = el("form"),
    log = el("div"),
    feedback = sub("");
  log.setAttribute("role", "log");
  const message = inputField(
    form,
    crypto.randomUUID(),
    "交代一件事",
    "",
    "textarea",
    true
  );
  const submit = el("button", "发送", "qo-primary");
  submit.type = "submit";
  let pending = null,
    sending = false;
  const samples = actions();
  [
    "本栋有什么规则",
    "新增约定：联系习惯｜晚间不主动打扰、不反复催促",
    "修改约定：联系习惯｜除紧急事项外，晚上九点后不主动联系",
    "停止约定：联系习惯",
  ].forEach((sample) =>
    samples.append(
      button(sample, () => {
        if (!pending) message.value = sample;
      })
    )
  );
  form.append(samples, actions(submit), feedback);
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    if (sending) return;
    if (!pending)
      pending = {
        scope,
        operation_id: crypto.randomUUID(),
        text: message.value.trim(),
      };
    sending = true;
    submit.disabled = true;
    message.readOnly = true;
    try {
      const result = await api("/api/foundation/talk", pending);
      log.append(el("p", `你：${pending.text}`), el("p", `二花：${result.reply}`));
      for (const attachment of result.attachments || []) {
        if (
          attachment.kind === "image" &&
          /^[0-9a-f-]{36}$/.test(attachment.artifact_ref || "")
        ) {
          const img = el("img");
          img.src = `/api/foundation/card?id=${encodeURIComponent(attachment.artifact_ref)}`;
          img.alt = attachment.label || "本次欢迎内容";
          img.className = "qo-review-image";
          log.append(img);
        } else if (attachment.kind === "text")
          log.append(el("p", attachment.text || ""));
      }
      if (result.suggestions?.length) {
        const choices = actions();
        result.suggestions.forEach((text) =>
          choices.append(
            button(text, () => {
              if (!pending) message.value = text;
            })
          )
        );
        log.append(choices);
      }
      pending = null;
      message.value = "";
      feedback.textContent = "页面与对话共用记录，切换标签可查看最新内容。";
      submit.textContent = "发送";
    } catch (error) {
      feedback.textContent = error.message;
      if (error.code) {
        pending = null;
      } else {
        submit.textContent = "核对原请求结果";
      }
    } finally {
      sending = false;
      submit.disabled = false;
      message.readOnly = Boolean(pending);
    }
  });
  const link = el("a", "打开完整本地验证对话");
  link.href = `/foundation?scope=${encodeURIComponent(scope)}`;
  chat.append(log, form, actions(link));
  settings.append(
    personalRulePanel(scope),
    stewardWelcomeSettings(scope),
    stewardDelegationPanel(scope)
  );
  settings.refreshWorkspace = () =>
    Array.from(settings.children).forEach((p) => p.refreshWorkspace?.());
  const matter = stewardProgress(scope);
  progress.append(matter);
  progress.refreshWorkspace = () => matter.refreshWorkspace?.();
  return workTabs("本栋工作", [
    ["与二花对话", chat],
    ["本栋知识", personalRulePanel(scope, "fact")],
    ["合作约定", settings],
    ["事项与进展", progress],
  ]);
}

function stewardDelegationPanel(scope) {
  const panel = box("临时审批代理"),
    status = sub("正在读取当前安排……"),
    content = el("div");
  panel.append(
    sub("在本栋当前在住人员中选择，临时替你审核欢迎卡片和文案。其他工作权限不变。"),
    status,
    content
  );
  let data,
    pending = false,
    frozen = null,
    dirty = false;
  async function load() {
    panel.inert = true;
    try {
      data = await api("/api/foundation/review-delegation", { scope });
      draw();
      dirty = false;
    } catch (e) {
      status.textContent = e.message;
    } finally {
      panel.inert = false;
    }
  }
  async function save(command) {
    if (pending || (frozen && command !== frozen)) return;
    pending = true;
    panel.inert = true;
    try {
      const r = await api("/api/foundation/review-delegation/change", command);
      frozen = null;
      await load();
      status.textContent = r.replayed
        ? "已核对原请求，未重复指定。"
        : r.status === "revoked"
          ? "已撤销代理，后续由你审核。"
          : "已保存代理安排，执行时仍核验在住状态与有效期。";
    } catch (e) {
      status.textContent = e.message;
      if (!e.code) {
        frozen = command;
        content.append(button("核对原请求结果", () => save(frozen)));
      }
    } finally {
      pending = false;
      panel.inert = false;
    }
  }
  function draw() {
    content.replaceChildren();
    const current = data.current;
    status.textContent = data.reason
      ? errors[data.reason] || "当前代理条件已失效，请重新安排或撤销。"
      : "";
    if (current)
      content.append(
        el("p", `当前代理：${current.label}`),
        sub(`${displayTime(current.valid_from)} 至 ${displayTime(current.valid_until)}`)
      );
    else content.append(sub("当前由你本人审核；选择直接发送的事项无需逐次审核。"));
    if (!data.can_designate) {
      content.append(sub("指定代理需要相应自主决定权。"));
      return;
    }
    const form = el("form"),
      prefix = crypto.randomUUID();
    form.addEventListener("input", () => {
      dirty = true;
    });
    const search = inputField(form, `${prefix}-search`, "搜索在住人员");
    const person = selectField(
      form,
      `${prefix}-person`,
      "代理人",
      data.candidates.map((p) => ({ id: p.person_ref, label: p.label })),
      "",
      "请选择本栋当前在住人员"
    );
    person.required = true;
    search.addEventListener("input", () => {
      const selected = person.value;
      person.replaceChildren(new Option("请选择本栋当前在住人员", ""));
      data.candidates
        .filter((p) => p.label.includes(search.value.trim()))
        .forEach((p) => person.add(new Option(p.label, p.person_ref)));
      person.value = selected;
    });
    if (!data.candidates.length)
      form.append(
        sub("暂无已核验的当前在住候选人。住宿来源未就绪或过期时不会显示可选名单。")
      );
    const from = inputField(
        form,
        `${prefix}-from`,
        "开始时间（留空即现在）",
        "",
        "datetime-local"
      ),
      until = inputField(
        form,
        `${prefix}-until`,
        "截止时间",
        "",
        "datetime-local",
        true
      );
    fieldExplanation(
      until,
      "代理的有效范围",
      "只代理本栋内容审核；不获得组织管理或其他发布权限。退住、到期或撤销后不能继续审批。未完成事项需要重新安排，不会自动直发。"
    );
    const submit = el("button", current ? "更换代理" : "指定代理", "qo-primary");
    submit.type = "submit";
    form.append(actions(submit, button("刷新在住名单", load)));
    form.addEventListener("submit", (e) => {
      e.preventDefault();
      if (frozen) {
        status.textContent = "请先核对原请求结果。";
        return;
      }
      save({
        scope,
        operation_id: crypto.randomUUID(),
        expected_id: current?.id || null,
        delegate: person.value,
        valid_from: from.value ? new Date(from.value).toISOString() : null,
        valid_until: new Date(until.value).toISOString(),
      });
    });
    content.append(form);
    if (current)
      content.append(
        button("撤销代理，恢复本人审核", () =>
          save({
            scope,
            operation_id: crypto.randomUUID(),
            expected_id: current.id,
            delegate: null,
            valid_from: null,
            valid_until: null,
          })
        )
      );
  }
  panel.refreshWorkspace = () => {
    if (!dirty && !frozen && !pending) load();
  };
  load();
  return panel;
}

function stewardWelcomeSettings(scope) {
  const panel = box("发布与审核约定"),
    status = sub("正在读取……"),
    content = el("div");
  let frozen = null,
    dirty = false;
  const localTime = (value) =>
    value
      ? new Date(
          new Date(value).getTime() - new Date(value).getTimezoneOffset() * 60000
        )
          .toISOString()
          .slice(0, 16)
      : "";
  panel.append(sub("在已有决定权内，安排具体事项先审还是直接发送。"), status, content);
  async function load() {
    panel.inert = true;
    try {
      const data = await api("/api/foundation/state", {});
      const targets = data.welcome.targets.filter((t) => t.scope_ref === scope);
      content.replaceChildren();
      dirty = false;
      status.textContent = "";
      if (!targets.length) {
        status.textContent = "当前没有已接入此范围的发布事项。";
        return;
      }
      targets.forEach((t) => {
        const form = el("form"),
          prefix = crypto.randomUUID();
        form.append(
          el("h4", "新居民入住欢迎"),
          sub(
            `目前：${t.rule?.content.mode === "direct" ? "按约定直接发" : t.rule ? "内容先审" : "尚未设置"}`
          )
        );
        const mode = selectField(
          form,
          `${prefix}-mode`,
          "内容做好后",
          [
            { id: "review", label: "先审核卡片和文案" },
            { id: "direct", label: "卡片和文案直接发送" },
          ],
          t.rule?.content.mode || "review"
        );
        fieldExplanation(
          mode,
          "直接发送的含义",
          "你的明确设置就是本次决定，不再重复请你确认。上层必要确认、实际入住、目标群成员和内容使用许可仍须满足。"
        );
        const applies = selectField(
          form,
          `${prefix}-applies`,
          "适用范围",
          [
            { id: "", label: "持续适用" },
            ...t.cases.map((c) => ({
              id: c.case_ref,
              label: `仅本次：${c.name || "待核验人员"}`,
            })),
          ],
          ""
        );
        const body = inputField(
          form,
          `${prefix}-text`,
          "欢迎文案",
          t.rule?.content.text_template || "欢迎 {name} 来到社区！",
          "textarea",
          true
        );
        const start = inputField(
            form,
            `${prefix}-start`,
            "开始时间（留空即现在）",
            "",
            "datetime-local"
          ),
          end = inputField(
            form,
            `${prefix}-end`,
            "结束时间（可不填）",
            "",
            "datetime-local"
          );
        fieldExplanation(
          end,
          "限时设置结束后",
          "期间设置结束后使用届时其他适用设置；仅本次不改变持续设置。没有适用设置时等待安排，不擅自发送。"
        );
        function fill() {
          const setting = applies.value
            ? t.case_rules.find((r) => r.case_ref === applies.value)
            : t.latest_rule || t.rule;
          mode.value = setting?.content.mode || t.rule?.content.mode || "review";
          body.value =
            setting?.content.text_template ||
            t.rule?.content.text_template ||
            "欢迎 {name} 来到社区！";
          start.value = localTime(setting?.effective_at);
          end.value = localTime(setting?.effective_until);
        }
        applies.addEventListener("change", fill);
        form.addEventListener("input", (e) => {
          if (e.target !== applies) dirty = true;
        });
        fill();
        const submit = el("button", "保存约定", "qo-primary");
        submit.type = "submit";
        submit.disabled = t.change_rules !== "autonomous" || Boolean(frozen);
        if (t.change_rules !== "autonomous")
          form.append(
            sub("当前账号没有自主修改这项发布约定的权限，请联系已获授权的负责人。")
          );
        async function send(command) {
          panel.inert = true;
          try {
            await api("/api/foundation/welcome", command);
            frozen = null;
            await load();
            status.textContent = "已保存，页面与二花使用同一设置；尚未因此发送消息。";
          } catch (e) {
            status.textContent = e.message;
            if (e.code) {
              frozen = null;
              submit.disabled = t.change_rules !== "autonomous";
            } else {
              submit.disabled = true;
              content.append(button("核对原请求", () => send(command)));
            }
          } finally {
            panel.inert = false;
          }
        }
        form.append(
          actions(
            submit,
            button("刷新当前设置", () => {
              if (frozen) {
                status.textContent = "请先核对原请求结果，草稿已保留。";
                return;
              }
              load();
            })
          )
        );
        form.addEventListener("submit", (e) => {
          e.preventDefault();
          if (frozen) return;
          const caseRef = applies.value || null;
          const prior = caseRef
            ? t.case_rules.find((r) => r.case_ref === caseRef)
            : null;
          frozen = {
            action: "configure",
            target_ref: t.target_ref,
            write: {
              scope,
              key: "resident_welcome",
              kind: "rule",
              shared: false,
              operation_id: crypto.randomUUID(),
              expected_version: caseRef ? prior?.version || 0 : t.latest_rule_version,
              case_ref: caseRef,
              content: {
                mode: mode.value,
                parts: ["text", "image"],
                phase: "formal",
                text_template: body.value.trim(),
              },
              effective_at: start.value ? new Date(start.value).toISOString() : null,
              effective_until: end.value ? new Date(end.value).toISOString() : null,
            },
          };
          send(frozen);
        });
        content.append(form);
      });
    } catch (e) {
      status.textContent = e.message;
    } finally {
      panel.inert = false;
    }
  }
  panel.refreshWorkspace = () => {
    if (!dirty && !frozen && !panel.inert) load();
  };
  load();
  return panel;
}

function stewardProgress(scope) {
  const panel = box("事项与进展"),
    status = sub("正在读取……"),
    list = el("div");
  panel.append(status, actions(button("刷新进展", load)), list);
  async function load() {
    try {
      const data = await api("/api/foundation/state", {});
      list.replaceChildren();
      status.textContent = "";
      for (const t of data.welcome.targets.filter(
        (t) => !scope || t.scope_ref === scope
      )) {
        for (const c of t.cases) {
          const item = box(`${c.name || "新居民"} · 入住欢迎`);
          item.append(sub(t.label));
          if (t.change_rules !== "denied" || t.publish !== "denied") {
            item.append(
              actions(
                button("准备并检查本次内容", () =>
                  act({
                    action: "prepare",
                    target_ref: t.target_ref,
                    case_ref: c.case_ref,
                    phase: "formal",
                  })
                ),
                button("继续办理（本地发送测试）", () =>
                  act({
                    action: "advance",
                    target_ref: t.target_ref,
                    case_ref: c.case_ref,
                    phase: "formal",
                  })
                )
              )
            );
            if (
              !t.artifacts.some(
                (a) => a.case_ref === c.case_ref && a.kind === "welcome_card"
              )
            )
              item.append(
                sub(
                  "卡片尚未生成。请在本地对话中使用“为本次欢迎制卡”，准备后在这里核对。"
                )
              );
          }
          for (const a of t.artifacts.filter((a) => a.case_ref === c.case_ref)) {
            const row = el("article", undefined, "qo-scope-row");
            if (a.kind === "welcome_card") {
              const img = el("img");
              img.src = `/api/foundation/card?id=${encodeURIComponent(a.artifact_ref)}`;
              img.alt = "本次待审欢迎卡片";
              img.className = "qo-review-image";
              row.append(img);
            } else row.append(el("p", a.text));
            const waiting = t.progress
              .flatMap((p) => p.waiting || [])
              .filter((w) => w.artifact_ref === a.artifact_ref);
            waiting.forEach((w) => {
              row.append(sub(errors[w.reason] || w.reason));
              if (w.reviewer === state.actor_person)
                row.append(
                  button("批准这个版本", () =>
                    act({
                      action: "approve",
                      target_ref: t.target_ref,
                      artifact_ref: a.artifact_ref,
                      target_version: a.target_version,
                      content_hash: a.content_hash,
                      phase: "formal",
                      approval_kind: w.approval_kind,
                      operation_id: crypto.randomUUID(),
                    })
                  )
                );
            });
            item.append(row);
          }
          t.actions
            .filter((a) => a.case_ref === c.case_ref)
            .forEach((a) =>
              item.append(
                sub(
                  `${a.part === "image" ? "卡片" : "文案"}：${{ succeeded: "本地发送测试成功", prepared: "已备妥", unknown: "结果待核对，请勿重发", cancelled: "已取消", retryable: "明确未发送" }[a.status] || a.status}`
                )
              )
            );
          list.append(item);
        }
      }
      if (!list.childElementCount) status.textContent = "当前没有需要展示的事项。";
    } catch (e) {
      status.textContent = e.message;
    }
  }
  async function act(command) {
    panel.inert = true;
    try {
      await api("/api/foundation/welcome", command);
      await load();
      status.textContent =
        command.action === "approve"
          ? "已记录对这个版本的批准，后续执行仍核验当前条件。"
          : "已更新办理进展；请查看每项内容的结果或等待原因。";
    } catch (e) {
      status.textContent = e.message;
    } finally {
      panel.inert = false;
    }
  }
  panel.refreshWorkspace = load;
  load();
  return panel;
}
