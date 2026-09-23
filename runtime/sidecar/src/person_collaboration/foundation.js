/* Local dialogue acceptance only. Actor identity and all effects belong to the service. */
(() => {
  "use strict";
  const $ = (id) => document.getElementById(id);
  let busy = false;
  const reasons = {
    authentication_required: "登录已失效，请重新登录。",
    scope_access_denied: "当前身份没有此范围的权限，请核对有效任职与授权。",
    access_denied: "当前身份无权执行此操作。",
    authority_changed_or_revoked: "相关权限已变化或撤销，本次执行已停止。",
    gateway_scope_mismatch: "本次对话只能处理已绑定的工作范围。",
    designated_confirmation_required: "还需要指定确认人批准这一具体版本。",
    executor_unavailable: "所需智能体当前未就绪，事项保留等待恢复。",
    knowledge_version_conflict: "约定已有更新，请先查询当前版本，再决定如何修改。",
    memory_version_conflict: "偏好已有更正，请先查询当前状态。",
    content_version_changed: "内容已经改版，旧批准不再适用。请查看当前内容。",
    pms_refresh_required: "住宿来源已过期，等待可信来源重新观测。",
    current_target_membership_required: "尚未确认本人已加入这个目标群。",
    outcome_unknown: "尚未取得明确回执，请先查看事项状态，不要重复执行。",
    foundation_disabled: "本地验收入口尚未启用。",
    foundation_unavailable: "本地服务暂时不可用。",
  };
  const node = (tag, text, cls) => {
    const result = document.createElement(tag);
    if (text !== undefined) result.textContent = text;
    if (cls) result.className = cls;
    return result;
  };
  function notice(text, error = false) {
    $("foundation-notice").textContent = text;
    $("foundation-notice").dataset.error = String(error);
  }
  function setBusy(value) {
    busy = value;
    $("foundation-main").setAttribute("aria-busy", String(value));
    document.querySelectorAll("button, #scope-select").forEach((el) => {
      el.disabled = value;
    });
  }
  async function request(path, body) {
    let response;
    try {
      response = await fetch(path, {
        method: body === undefined ? "GET" : "POST",
        credentials: "same-origin",
        headers: body === undefined ? {} : { "Content-Type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch {
      throw new Error("outcome_unknown");
    }
    if (response.status === 401) throw new Error("authentication_required");
    let result;
    try {
      result = await response.json();
    } catch {
      throw new Error("foundation_unavailable");
    }
    if (!response.ok || result.ok === false)
      throw new Error(result.error?.code || result.code || "foundation_unavailable");
    return result.ok === true && "result" in result ? result.result : result;
  }
  function choice(text) {
    const button = node("button", text);
    button.type = "button";
    button.addEventListener("click", () => {
      $("talk-input").value = text;
      $("talk-input").focus();
    });
    return button;
  }
  function entry(who, text, self = false, response = null) {
    const log = $("talk-log");
    log.querySelector(".pf-empty")?.remove();
    const item = node(
      "article",
      undefined,
      `pf-talk-entry ${self ? "pf-self" : "pf-agent"}`
    );
    item.append(node("strong", who), node("div", text));
    for (const attachment of response?.attachments || []) {
      const figure = node("figure");
      figure.append(node("figcaption", attachment.label || "本次事项内容"));
      if (
        attachment.kind === "image" &&
        /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(
          attachment.artifact_ref
        )
      ) {
        const image = node("img");
        image.src = `/api/foundation/card?id=${attachment.artifact_ref}`;
        image.alt = attachment.label || "待核对的具体内容";
        image.className = "pf-card-image";
        figure.append(image);
      } else if (attachment.kind === "text")
        figure.append(node("p", attachment.text || ""));
      else continue;
      item.append(figure);
    }
    if (response?.suggestions?.length) {
      const choices = node("div", undefined, "pf-actions");
      for (const text of response.suggestions)
        if (typeof text === "string" && text.length <= 500)
          choices.append(choice(text));
      item.append(choices);
    }
    log.append(item);
    item.scrollIntoView({ block: "nearest" });
  }
  for (const text of [
    "认识我吗",
    "以后回答我简短一点",
    "涉及费用还是详细说清楚",
    "以后不用记我的回复习惯",
    "本栋有什么规则",
    "新增约定：公共空间使用｜使用后请归位清洁",
    "修改约定：公共空间使用｜使用后请归位并关闭照明",
    "停止约定：公共空间使用",
    "把本栋厨房关闭时间改成晚上十点",
  ])
    $("talk-samples").append(choice(text));
  $("talk-form").addEventListener("submit", async (event) => {
    event.preventDefault();
    const text = $("talk-input").value.trim();
    if (busy || !text || !$("scope-select").value) return;
    entry("我", text, true);
    setBusy(true);
    try {
      const result = await request("/api/foundation/talk", {
        scope: $("scope-select").value,
        text,
        operation_id: crypto.randomUUID(),
      });
      entry("二花", result.reply || "已处理，请查询当前事项状态。", false, result);
      $("talk-input").value = "";
      notice("已读取持久服务的实际回执。");
    } catch (error) {
      const message =
        reasons[error.message] ||
        "当前条件不允许继续，请查询事项状态，核对身份、权限与内容版本。";
      notice(message, true);
      entry("二花", message);
    } finally {
      setBusy(false);
    }
  });
  $("scope-select").addEventListener("change", () => {
    notice("本次对话范围已切换，后续操作将重新核验当前授权。");
  });
  setBusy(true);
  request("/api/foundation/state")
    .then((state) => {
      $("current-person").textContent =
        `${state.identity?.preferred_name || "当前体验账号"} · 已核验`;
      $("scope-select").replaceChildren();
      for (const scope of state.configuration?.scopes || [])
        $("scope-select").add(
          new Option(scope.label || scope.name || "已获准范围", scope.id)
        );
      if (!$("scope-select").options.length)
        $("scope-select").add(new Option("暂无获准工作范围", ""));
      notice("当前身份与范围已读取，可开始对话验收。");
    })
    .catch((error) => notice(reasons[error.message] || "本地服务暂时不可用。", true))
    .finally(() => setBusy(false));
})();
